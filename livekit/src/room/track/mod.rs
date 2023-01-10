use crate::proto::{TrackSource as ProtoTrackSource, TrackType};
use crate::room::id::TrackSid;
use livekit_utils::enum_dispatch;
use livekit_utils::observer::Dispatcher;
use livekit_webrtc::media_stream::{MediaStreamTrackHandle, MediaStreamTrackTrait};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

pub mod audio_track;
pub mod local_audio_track;
pub mod local_track;
pub mod local_video_track;
pub mod remote_audio_track;
pub mod remote_track;
pub mod remote_video_track;
pub mod video_track;

pub use audio_track::*;
pub use local_audio_track::*;
pub use local_track::*;
pub use local_video_track::*;
pub use remote_audio_track::*;
pub use remote_track::*;
pub use remote_video_track::*;
pub use video_track::*;

#[derive(Debug)]
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

impl From<TrackType> for TrackKind {
    fn from(r#type: TrackType) -> Self {
        match r#type {
            TrackType::Audio => Self::Audio,
            TrackType::Video => Self::Video,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
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

impl From<ProtoTrackSource> for TrackSource {
    fn from(source: ProtoTrackSource) -> Self {
        match source {
            ProtoTrackSource::Camera => Self::Camera,
            ProtoTrackSource::Microphone => Self::Microphone,
            ProtoTrackSource::ScreenShare => Self::Screenshare,
            ProtoTrackSource::ScreenShareAudio => Self::ScreenshareAudio,
            ProtoTrackSource::Unknown => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TrackDimension(pub u32, pub u32);

pub trait TrackTrait {
    fn sid(&self) -> TrackSid;
    fn name(&self) -> String;
    fn kind(&self) -> TrackKind;
    fn stream_state(&self) -> StreamState;
    fn start(&self);
    fn stop(&self);
    fn register_observer(&self) -> mpsc::UnboundedReceiver<TrackEvent>;
    fn set_muted(&self, muted: bool);
}

#[derive(Debug, Clone)]
pub enum TrackEvent {
    Mute,
    Unmute,
}

#[derive(Debug)]
pub(super) struct TrackShared {
    pub(super) sid: Mutex<TrackSid>,
    pub(super) name: Mutex<String>,
    pub(super) kind: AtomicU8,         // TrackKind
    pub(super) stream_state: AtomicU8, // StreamState
    pub(super) muted: AtomicBool,
    pub(super) rtc_track: MediaStreamTrackHandle,
    pub(super) dispatcher: Mutex<Dispatcher<TrackEvent>>,
}

impl TrackShared {
    pub(crate) fn new(
        sid: TrackSid,
        name: String,
        kind: TrackKind,
        rtc_track: MediaStreamTrackHandle,
    ) -> Self {
        Self {
            sid: Mutex::new(sid),
            name: Mutex::new(name),
            kind: AtomicU8::new(kind as u8),
            stream_state: AtomicU8::new(StreamState::Active as u8),
            muted: AtomicBool::new(false),
            rtc_track,
            dispatcher: Default::default(),
        }
    }

    pub(crate) fn start(&self) {
        self.rtc_track.set_enabled(true);
    }

    pub(crate) fn stop(&self) {
        self.rtc_track.set_enabled(false);
    }

    pub(crate) fn set_muted(&self, muted: bool) {
        if self.muted.load(Ordering::SeqCst) == muted {
            return;
        }

        self.muted.store(muted, Ordering::SeqCst);
        self.rtc_track.set_enabled(!muted);

        self.dispatcher.lock().dispatch(if muted {
            &TrackEvent::Mute
        } else {
            &TrackEvent::Unmute
        });
    }

    pub(crate) fn register_observer(&self) -> mpsc::UnboundedReceiver<TrackEvent> {
        self.dispatcher.lock().register()
    }
}

#[derive(Clone, Debug)]
pub enum TrackHandle {
    LocalVideo(Arc<LocalVideoTrack>),
    LocalAudio(Arc<LocalAudioTrack>),
    RemoteVideo(Arc<RemoteVideoTrack>),
    RemoteAudio(Arc<RemoteAudioTrack>),
}

impl TrackTrait for TrackHandle {
    enum_dispatch!(
        [LocalVideo, LocalAudio, RemoteVideo, RemoteAudio]
        fnc!(sid, &Self, [], TrackSid);
        fnc!(name, &Self, [], String);
        fnc!(kind, &Self, [], TrackKind);
        fnc!(stream_state, &Self, [], StreamState);
        fnc!(start, &Self, [], ());
        fnc!(stop, &Self, [], ());
        fnc!(register_observer, &Self, [], mpsc::UnboundedReceiver<TrackEvent>);
        fnc!(set_muted, &Self, [muted: bool], ());
    );
}

impl TrackHandle {
    pub fn rtc_track(&self) -> MediaStreamTrackHandle {
        match self {
            Self::RemoteVideo(remote_video) => {
                MediaStreamTrackHandle::Video(remote_video.rtc_track())
            }
            Self::RemoteAudio(remote_audio) => {
                MediaStreamTrackHandle::Audio(remote_audio.rtc_track())
            }
            _ => todo!(),
        }
    }
}

macro_rules! impl_track_trait {
    ($x:ident) => {
        use std::sync::atomic::Ordering;
        use tokio::sync::mpsc;
        use $crate::room::id::TrackSid;
        use $crate::room::track::{StreamState, TrackEvent, TrackKind, TrackTrait};

        impl TrackTrait for $x {
            fn sid(&self) -> TrackSid {
                self.shared.sid.lock().clone()
            }

            fn name(&self) -> String {
                self.shared.name.lock().clone()
            }

            fn kind(&self) -> TrackKind {
                self.shared.kind.load(Ordering::SeqCst).into()
            }

            fn stream_state(&self) -> StreamState {
                self.shared.stream_state.load(Ordering::SeqCst).into()
            }

            fn start(&self) {
                self.shared.start();
            }

            fn stop(&self) {
                self.shared.stop();
            }

            fn register_observer(&self) -> mpsc::UnboundedReceiver<TrackEvent> {
                self.shared.register_observer()
            }

            fn set_muted(&self, muted: bool) {
                self.shared.set_muted(muted);
            }
        }
    };
}

pub(super) use impl_track_trait;
