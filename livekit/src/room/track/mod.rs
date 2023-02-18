use crate::prelude::*;
use crate::proto;
use livekit_utils::enum_dispatch;
use livekit_utils::observer::Dispatcher;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use thiserror::Error;
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

pub trait Track<T>
where
    T: MediaStreamTrackTrait,
{
    fn sid(&self) -> TrackSid;
    fn name(&self) -> String;
    fn kind(&self) -> TrackKind;
    fn source(&self) -> TrackSource;
    fn stream_state(&self) -> StreamState;
    fn muted(&self) -> bool;
    fn start(&self);
    fn stop(&self);
    fn register_observer(&self) -> mpsc::UnboundedReceiver<TrackEvent>;
    fn set_muted(&self, muted: bool);
    fn rtc_track(&self) -> T;
}

pub trait LocalTrack: Track {}

pub trait RemoteTrack: Track {}

pub trait AudioTrack: Track {}

pub trait VideoTrack: Track {}

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

pub trait Track<T>
where
    T: MediaStreamTrackTrait,
{
    fn sid(&self) -> TrackSid;
    fn name(&self) -> String;
    fn kind(&self) -> TrackKind;
    fn source(&self) -> TrackSource;
    fn stream_state(&self) -> StreamState;
    fn muted(&self) -> bool;
    fn start(&self);
    fn stop(&self);
    fn register_observer(&self) -> mpsc::UnboundedReceiver<TrackEvent>;
    fn set_muted(&self, muted: bool);
    fn rtc_track(&self) -> T;
}

pub trait VideoTrack: Track<VideoTrack> {}

pub(crate) trait TrackInternalTrait {
    fn update_muted(&self, muted: bool, dispatch: bool);
    fn update_source(&self, source: TrackSource);
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
    pub(super) source: AtomicU8,       // TrackSource
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
            source: AtomicU8::new(TrackSource::Unknown as u8),
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

    pub(crate) fn register_observer(&self) -> mpsc::UnboundedReceiver<TrackEvent> {
        self.dispatcher.lock().register()
    }
}

impl TrackInternalTrait for TrackShared {
    fn update_muted(&self, muted: bool, dispatch: bool) {
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

    fn update_source(&self, source: TrackSource) {
        self.source.store(source as u8, Ordering::SeqCst);
    }
}

#[derive(Clone, Debug)]
pub enum TrackHandle {
    LocalVideo(Arc<LocalVideoTrack>),
    LocalAudio(Arc<LocalAudioTrack>),
    RemoteVideo(Arc<RemoteVideoTrack>),
    RemoteAudio(Arc<RemoteAudioTrack>),
}

impl TrackTrait<MediaStreamTrackHandle> for TrackHandle {
    enum_dispatch!(
        [LocalVideo, LocalAudio, RemoteVideo, RemoteAudio]
        fnc!(sid, &Self, [], TrackSid);
        fnc!(name, &Self, [], String);
        fnc!(kind, &Self, [], TrackKind);
        fnc!(source, &Self, [], TrackSource);
        fnc!(stream_state, &Self, [], StreamState);
        fnc!(muted, &Self, [], bool);
        fnc!(start, &Self, [], ());
        fnc!(stop, &Self, [], ());
        fnc!(register_observer, &Self, [], mpsc::UnboundedReceiver<TrackEvent>);
        fnc!(set_muted, &Self, [muted: bool], ());
        fnc!(rtc_track, &Self, [], MediaStreamTrackHandle);
    );
}

macro_rules! impl_track_trait {
    ($x:ident, $rtc_track:ty) => {
        use std::sync::atomic::Ordering;
        use tokio::sync::mpsc;
        use $crate::room::id::TrackSid;
        use $crate::room::track::{StreamState, TrackEvent, TrackKind, TrackSource, TrackTrait, TrackInternalTrait};

        impl TrackTrait<$rtc_track> for $x {
            fn sid(&self) -> TrackSid {
                self.shared.sid.lock().clone()
            }

            fn name(&self) -> String {
                self.shared.name.lock().clone()
            }

            fn kind(&self) -> TrackKind {
                self.shared.kind.load(Ordering::SeqCst).into()
            }

            fn source(&self) -> TrackSource {
                self.shared.source.load(Ordering::SeqCst).into()
            }

            fn stream_state(&self) -> StreamState {
                self.shared.stream_state.load(Ordering::SeqCst).into()
            }

            fn muted(&self) -> bool {
                self.shared.muted.load(Ordering::SeqCst)
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
                self.shared.update_muted(muted, true);
            }
        }

        impl TrackInternalTrait for $x {
            fn update_muted(&self, muted: bool, dispatch: bool) {
                self.shared.update_muted(muted, dispatch);
            }

            fn update_source(&self, source: TrackSource) {
                self.shared.update_source(source);
            }
        }
    };
    ($x:ident, $rtc_track:ty, enum_dispatch, [$($variant:ident),+]) => {
        use livekit_utils::enum_dispatch;
        use $crate::room::track::{TrackTrait, TrackInternalTrait};
        use tokio::sync::mpsc;

        impl TrackTrait<$rtc_track> for $x {
            enum_dispatch!(
                [$($variant),+]
                fnc!(sid, &Self, [], TrackSid);
                fnc!(name, &Self, [], String);
                fnc!(kind, &Self, [], TrackKind);
                fnc!(source, &Self, [], TrackSource);
                fnc!(stream_state, &Self, [], StreamState);
                fnc!(muted, &Self, [], bool);
                fnc!(start, &Self, [], ());
                fnc!(stop, &Self, [], ());
                fnc!(register_observer, &Self, [], mpsc::UnboundedReceiver<TrackEvent>);
                fnc!(set_muted, &Self, [muted: bool], ());
                fnc!(rtc_track, &Self, [], $rtc_track);
            );
        }

        impl TrackInternalTrait for $x {
            enum_dispatch!(
                [$($variant),+]
                fnc!(update_muted, &Self, [muted: bool, dispatch: bool], ());
                fnc!(update_source, &Self, [source: TrackSource], ());
            );

        }
    };
}

pub(super) use impl_track_trait;
