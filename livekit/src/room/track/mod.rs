use crate::prelude::*;
use crate::proto;
use livekit_utils::enum_dispatch;
use livekit_utils::observer::Dispatcher;
use livekit_webrtc as rtc;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc;
use thiserror::Error;

pub mod local_audio_track;
pub mod local_video_track;
pub mod remote_audio_track;
pub mod remote_video_track;

pub use local_audio_track::*;
pub use local_video_track::*;
pub use remote_audio_track::*;
pub use remote_video_track::*;

#[derive(Error, Debug, Clone)]
pub enum TrackError {
    #[error("could not find published track with sid: {0}")]
    TrackNotFound(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Unknown,
    Audio,
    Video,
}

impl From<u8> for TrackKind {
    fn from(val: u8) -> Self {
        match val {
            1 => Self::Audio,
            2 => Self::Video,
            _ => Self::Unknown,
        }
    }
}

impl From<proto::TrackType> for TrackKind {
    fn from(r#type: proto::TrackType) -> Self {
        match r#type {
            proto::TrackType::Audio => Self::Audio,
            proto::TrackType::Video => Self::Video,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Unknown,
    Active,
    Paused,
}

impl From<u8> for StreamState {
    fn from(val: u8) -> Self {
        match val {
            1 => Self::Active,
            2 => Self::Paused,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackSource {
    Unknown,
    Camera,
    Microphone,
    Screenshare,
    ScreenshareAudio,
}

impl From<u8> for TrackSource {
    fn from(val: u8) -> Self {
        match val {
            1 => Self::Camera,
            2 => Self::Microphone,
            3 => Self::Screenshare,
            4 => Self::ScreenshareAudio,
            _ => Self::Unknown,
        }
    }
}

impl From<proto::TrackSource> for TrackSource {
    fn from(source: proto::TrackSource) -> Self {
        match source {
            proto::TrackSource::Camera => Self::Camera,
            proto::TrackSource::Microphone => Self::Microphone,
            proto::TrackSource::ScreenShare => Self::Screenshare,
            proto::TrackSource::ScreenShareAudio => Self::ScreenshareAudio,
            proto::TrackSource::Unknown => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TrackDimension(pub u32, pub u32);

#[derive(Debug, Clone)]
pub enum TrackEvent {
    Mute,
    Unmute,
}

#[derive(Clone)]
pub enum Track {
    LocalAudio(LocalAudioTrack),
    LocalVideo(LocalVideoTrack),
    RemoteAudio(RemoteAudioTrack),
    RemoteVideo(RemoteVideoTrack),
}

#[derive(Clone)]
pub enum LocalTrack {
    Audio(LocalAudioTrack),
    Video(LocalVideoTrack),
}

#[derive(Clone)]
pub enum RemoteTrack {
    Audio(RemoteAudioTrack),
    Video(RemoteVideoTrack),
}

#[derive(Clone)]
pub enum VideoTrack {
    Local(LocalVideoTrack),
    Remote(RemoteVideoTrack),
}

#[derive(Clone)]
pub enum AudioTrack {
    Local(LocalAudioTrack),
    Remote(RemoteAudioTrack),
}

macro_rules! track_dispatch {
    ([$($variant:ident),+]) => {
        enum_dispatch!(
            [$($variant),+];
            pub fn sid(self: &Self) -> TrackSid;
            pub fn name(self: &Self) -> String;
            pub fn kind(self: &Self) -> TrackKind;
            pub fn source(self: &Self) -> TrackSource;
            pub fn stream_state(self: &Self) -> StreamState;
            pub fn start(self: &Self) -> ();
            pub fn stop(self: &Self) -> ();
            pub fn muted(self: &Self) -> bool;
            pub fn set_muted(self: &Self, muted: bool) -> ();
            pub fn register_observer(self: &Self) -> mpsc::Receiver<TrackEvent>;
        );
    };
}

impl Track {
    track_dispatch!([LocalAudio, LocalVideo, RemoteAudio, RemoteVideo]);

    pub fn rtc_track(&self) -> rtc::media_stream::MediaStreamTrack {
        match self {
            Self::LocalAudio(track) => track.inner.rtc_track(),
            Self::LocalVideo(track) => track.inner.rtc_track(),
            Self::RemoteAudio(track) => track.inner.rtc_track(),
            Self::RemoteVideo(track) => track.inner.rtc_track(),
        }
    }
}

impl LocalTrack {
    track_dispatch!([Audio, Video]);

    pub fn rtc_track(&self) -> rtc::media_stream::MediaStreamTrack {
        match self {
            Self::Audio(track) => track.inner.rtc_track(),
            Self::Video(track) => track.inner.rtc_track(),
        }
    }
}

impl RemoteTrack {
    track_dispatch!([Audio, Video]);

    pub fn rtc_track(&self) -> rtc::media_stream::MediaStreamTrack {
        match self {
            Self::Audio(track) => track.inner.rtc_track(),
            Self::Video(track) => track.inner.rtc_track(),
        }
    }
}

impl VideoTrack {
    track_dispatch!([Local, Remote]);

    pub fn rtc_track(&self) -> rtc::media_stream::VideoTrack {
        match self {
            Self::Local(track) => track.rtc_track(),
            Self::Remote(track) => track.rtc_track(),
        }
    }
}

impl AudioTrack {
    track_dispatch!([Local, Remote]);

    pub fn rtc_track(&self) -> rtc::media_stream::AudioTrack {
        match self {
            Self::Local(track) => track.rtc_track(),
            Self::Remote(track) => track.rtc_track(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct TrackInner {
    pub sid: Mutex<TrackSid>,
    pub name: Mutex<String>,
    pub kind: AtomicU8,         // TrackKind
    pub source: AtomicU8,       // TrackSource
    pub stream_state: AtomicU8, // StreamState
    pub muted: AtomicBool,
    pub rtc_track: rtc::media_stream::MediaStreamTrack,
    pub dispatcher: Mutex<Dispatcher<TrackEvent>>,
}

impl TrackInner {
    pub fn new(
        sid: TrackSid,
        name: String,
        kind: TrackKind,
        rtc_track: rtc::media_stream::MediaStreamTrack,
    ) -> Self {
        Self {
            sid: Mutex::new(sid),
            name: Mutex::new(name),
            kind: AtomicU8::new(kind as u8),
            source: AtomicU8::new(TrackSource::Unknown as u8),
            stream_state: AtomicU8::new(StreamState::Active as u8),
            muted: AtomicBool::new(false),
            rtc_track,
            dispatcher: Default::default(),
        }
    }

    pub fn sid(&self) -> TrackSid {
        self.sid.lock().clone()
    }

    pub fn name(&self) -> String {
        self.name.lock().clone()
    }

    pub fn kind(&self) -> TrackKind {
        self.kind.load(Ordering::SeqCst).into()
    }

    pub fn source(&self) -> TrackSource {
        self.source.load(Ordering::SeqCst).into()
    }

    pub fn stream_state(&self) -> StreamState {
        self.stream_state.load(Ordering::SeqCst).into()
    }

    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::SeqCst)
    }

    pub fn start(&self) {
        self.rtc_track.set_enabled(true);
    }

    pub fn stop(&self) {
        self.rtc_track.set_enabled(false);
    }

    pub fn set_muted(&self, muted: bool) {
        if self
            .muted
            .compare_exchange(!muted, muted, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        if !muted {
            self.start();
        } else {
            self.stop();
        }

        let event = if muted {
            TrackEvent::Mute
        } else {
            TrackEvent::Unmute
        };

        self.dispatcher.lock().dispatch(&event);
    }

    pub fn set_source(&self, source: TrackSource) {
        self.source.store(source as u8, Ordering::SeqCst);
    }

    pub fn rtc_track(&self) -> rtc::media_stream::MediaStreamTrack {
        self.rtc_track.clone()
    }

    pub fn register_observer(&self) -> mpsc::Receiver<TrackEvent> {
        self.dispatcher.lock().register()
    }
}
