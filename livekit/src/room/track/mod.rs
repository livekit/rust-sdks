use crate::prelude::*;
use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use livekit_protocol::observer::Dispatcher;
use livekit_webrtc::prelude::*;
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use thiserror::Error;
use tokio::sync::mpsc;

mod local_audio_track;
mod local_video_track;
mod remote_audio_track;
mod remote_video_track;

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
    Audio,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Active,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackSource {
    Unknown,
    Camera,
    Microphone,
    Screenshare,
    ScreenshareAudio,
}

#[derive(Debug, Clone)]
pub enum TrackEvent {
    Muted,
    Unmuted,
}

#[derive(Clone, Copy, Debug)]
pub struct TrackDimension(pub u32, pub u32);

#[derive(Clone, Debug)]
pub enum Track {
    LocalAudio(LocalAudioTrack),
    LocalVideo(LocalVideoTrack),
    RemoteAudio(RemoteAudioTrack),
    RemoteVideo(RemoteVideoTrack),
}

#[derive(Clone, Debug)]
pub enum LocalTrack {
    Audio(LocalAudioTrack),
    Video(LocalVideoTrack),
}

#[derive(Clone, Debug)]
pub enum RemoteTrack {
    Audio(RemoteAudioTrack),
    Video(RemoteVideoTrack),
}

#[derive(Clone, Debug)]
pub enum VideoTrack {
    Local(LocalVideoTrack),
    Remote(RemoteVideoTrack),
}

#[derive(Clone, Debug)]
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
            pub fn is_muted(self: &Self) -> bool;
            pub fn is_remote(self: &Self) -> bool;
            pub fn register_observer(self: &Self) -> mpsc::UnboundedReceiver<TrackEvent>;

            pub(crate) fn transceiver(self: &Self) -> Option<RtpTransceiver>;
            pub(crate) fn update_transceiver(self: &Self, transceiver: Option<RtpTransceiver>) -> ();
            pub(crate) fn update_info(self: &Self, info: proto::TrackInfo) -> ();
        );
    };
}

impl Track {
    track_dispatch!([LocalAudio, LocalVideo, RemoteAudio, RemoteVideo]);

    #[inline]
    pub fn rtc_track(&self) -> MediaStreamTrack {
        match self {
            Self::LocalAudio(track) => track.rtc_track().into(),
            Self::LocalVideo(track) => track.rtc_track().into(),
            Self::RemoteAudio(track) => track.rtc_track().into(),
            Self::RemoteVideo(track) => track.rtc_track().into(),
        }
    }
}

impl LocalTrack {
    track_dispatch!([Audio, Video]);

    #[inline]
    pub fn rtc_track(&self) -> MediaStreamTrack {
        match self {
            Self::Audio(track) => track.rtc_track().into(),
            Self::Video(track) => track.rtc_track().into(),
        }
    }
}

impl RemoteTrack {
    track_dispatch!([Audio, Video]);

    #[inline]
    pub fn rtc_track(&self) -> MediaStreamTrack {
        match self {
            Self::Audio(track) => track.rtc_track().into(),
            Self::Video(track) => track.rtc_track().into(),
        }
    }
}

impl VideoTrack {
    track_dispatch!([Local, Remote]);

    #[inline]
    pub fn rtc_track(&self) -> RtcVideoTrack {
        match self {
            Self::Local(track) => track.rtc_track(),
            Self::Remote(track) => track.rtc_track(),
        }
    }
}

impl AudioTrack {
    track_dispatch!([Local, Remote]);

    #[inline]
    pub fn rtc_track(&self) -> RtcAudioTrack {
        match self {
            Self::Local(track) => track.rtc_track().into(),
            Self::Remote(track) => track.rtc_track().into(),
        }
    }
}

#[derive(Debug)]
struct TrackInfo {
    sid: TrackSid,
    name: String,
    kind: TrackKind,
    source: TrackSource,
    stream_state: StreamState,
    muted: bool,
    transceiver: Option<RtpTransceiver>,
}

#[derive(Debug)]
pub(crate) struct TrackInner {
    pub info: RwLock<TrackInfo>,
    pub rtc_track: MediaStreamTrack,
    pub dispatcher: Dispatcher<TrackEvent>,
}

impl TrackInner {
    pub fn new(sid: TrackSid, name: String, kind: TrackKind, rtc_track: MediaStreamTrack) -> Self {
        Self {
            info: RwLock::new(TrackInfo {
                sid,
                name,
                kind,
                source: TrackSource::Unknown,
                stream_state: StreamState::Active,
                muted: false,
                transceiver: None,
            }),
            rtc_track,
            dispatcher: Default::default(),
        }
    }

    pub fn sid(&self) -> TrackSid {
        self.info.read().sid.clone()
    }

    pub fn name(&self) -> String {
        self.info.read().name.clone()
    }

    pub fn kind(&self) -> TrackKind {
        self.info.read().kind
    }

    pub fn source(&self) -> TrackSource {
        self.info.read().source
    }

    pub fn stream_state(&self) -> StreamState {
        self.info.read().stream_state
    }

    pub fn is_muted(&self) -> bool {
        self.info.read().muted
    }

    pub fn enable(&self) {
        self.rtc_track.set_enabled(true);
    }

    pub fn disable(&self) {
        self.rtc_track.set_enabled(false);
    }

    // Should only be called on a LocalTrack
    pub fn set_muted(&self, muted: bool) {
        if self
            .muted
            .compare_exchange(!muted, muted, Ordering::Acquire, Ordering::Release)
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

        self.dispatcher.dispatch(&event);
    }

    pub fn rtc_track(&self) -> MediaStreamTrack {
        self.rtc_track.clone()
    }

    pub fn register_observer(&self) -> mpsc::UnboundedReceiver<TrackEvent> {
        self.dispatcher.register()
    }

    pub fn transceiver(&self) -> Option<RtpTransceiver> {
        self.info.read().transceiver.clone()
    }

    pub fn update_transceiver(&self, transceiver: Option<RtpTransceiver>) {
        self.info.write().transceiver = transceiver;
    }

    pub fn update_info(&self, new_info: proto::TrackInfo) {
        let mut info = self.info.write();
        info.name = new_info.name;
        info.sid = new_info.sid.into();
        info.kind =
            TrackKind::try_from(proto::TrackType::from_i32(new_info.r#type).unwrap()).unwrap();
        info.source = TrackSource::from(proto::TrackSource::from_i32(new_info.source).unwrap());
        // Muted and StreamState are not handled separately (events)
    }
}

impl From<RemoteTrack> for Track {
    fn from(track: RemoteTrack) -> Self {
        match track {
            RemoteTrack::Audio(track) => Self::RemoteAudio(track),
            RemoteTrack::Video(track) => Self::RemoteVideo(track),
        }
    }
}

impl From<LocalTrack> for Track {
    fn from(track: LocalTrack) -> Self {
        match track {
            LocalTrack::Audio(track) => Self::LocalAudio(track),
            LocalTrack::Video(track) => Self::LocalVideo(track),
        }
    }
}

impl From<VideoTrack> for Track {
    fn from(track: VideoTrack) -> Self {
        match track {
            VideoTrack::Local(track) => Self::LocalVideo(track),
            VideoTrack::Remote(track) => Self::RemoteVideo(track),
        }
    }
}

impl From<AudioTrack> for Track {
    fn from(track: AudioTrack) -> Self {
        match track {
            AudioTrack::Local(track) => Self::LocalAudio(track),
            AudioTrack::Remote(track) => Self::RemoteAudio(track),
        }
    }
}

impl TryFrom<Track> for RemoteTrack {
    type Error = &'static str;

    fn try_from(track: Track) -> Result<Self, Self::Error> {
        match track {
            Track::RemoteAudio(track) => Ok(Self::Audio(track)),
            Track::RemoteVideo(track) => Ok(Self::Video(track)),
            _ => Err("not a remote track"),
        }
    }
}

impl TryFrom<Track> for LocalTrack {
    type Error = &'static str;

    fn try_from(track: Track) -> Result<Self, Self::Error> {
        match track {
            Track::LocalAudio(track) => Ok(Self::Audio(track)),
            Track::LocalVideo(track) => Ok(Self::Video(track)),
            _ => Err("not a local track"),
        }
    }
}

impl TryFrom<Track> for VideoTrack {
    type Error = &'static str;

    fn try_from(track: Track) -> Result<Self, Self::Error> {
        match track {
            Track::LocalVideo(track) => Ok(Self::Local(track)),
            Track::RemoteVideo(track) => Ok(Self::Remote(track)),
            _ => Err("not a video track"),
        }
    }
}

impl TryFrom<Track> for AudioTrack {
    type Error = &'static str;

    fn try_from(track: Track) -> Result<Self, Self::Error> {
        match track {
            Track::LocalAudio(track) => Ok(Self::Local(track)),
            Track::RemoteAudio(track) => Ok(Self::Remote(track)),
            _ => Err("not an audio track"),
        }
    }
}

// Conversions from integers (Useful since we're using atomic values to represent our enums)

impl TryFrom<u8> for TrackKind {
    type Error = &'static str;

    fn try_from(kind: u8) -> Result<Self, Self::Error> {
        match kind {
            0 => Ok(Self::Audio),
            1 => Ok(Self::Video),
            _ => Err("invalid track kind"),
        }
    }
}

impl TryFrom<u8> for StreamState {
    type Error = &'static str;

    fn try_from(state: u8) -> Result<Self, Self::Error> {
        match state {
            0 => Ok(Self::Active),
            1 => Ok(Self::Paused),
            _ => Err("invalid stream state"),
        }
    }
}

impl From<u8> for TrackSource {
    fn from(source: u8) -> Self {
        match source {
            1 => Self::Camera,
            2 => Self::Microphone,
            3 => Self::Screenshare,
            4 => Self::ScreenshareAudio,
            _ => Self::Unknown,
        }
    }
}

impl From<TrackKind> for MediaType {
    fn from(kind: TrackKind) -> Self {
        match kind {
            TrackKind::Audio => Self::Audio,
            TrackKind::Video => Self::Video,
        }
    }
}
