use crate::prelude::*;
use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use livekit_webrtc::prelude::*;
use parking_lot::RwLock;
use std::{fmt::Debug, sync::Arc};
use thiserror::Error;

mod audio_track;
mod local_audio_track;
mod local_track;
mod local_video_track;
mod remote_audio_track;
mod remote_track;
mod remote_video_track;
mod video_track;

pub use audio_track::*;
pub use local_audio_track::*;
pub use local_track::*;
pub use local_video_track::*;
pub use remote_audio_track::*;
pub use remote_track::*;
pub use remote_video_track::*;
pub use video_track::*;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackDimension(pub u32, pub u32);

macro_rules! track_dispatch {
    ([$($variant:ident),+]) => {
        enum_dispatch!(
            [$($variant),+];
            pub fn sid(self: &Self) -> TrackSid;
            pub fn name(self: &Self) -> String;
            pub fn kind(self: &Self) -> TrackKind;
            pub fn source(self: &Self) -> TrackSource;
            pub fn stream_state(self: &Self) -> StreamState;
            pub fn enable(self: &Self) -> ();
            pub fn disable(self: &Self) -> ();
            pub fn is_muted(self: &Self) -> bool;
            pub fn is_remote(self: &Self) -> bool;

            /*pub(crate) fn transceiver(self: &Self) -> Option<RtpTransceiver>;
            pub(crate) fn update_transceiver(self: &Self, transceiver: Option<RtpTransceiver>) -> ();
            pub(crate) fn update_info(self: &Self, info: proto::TrackInfo) -> ();*/
        );
    };
}

#[derive(Clone, Debug)]
pub enum Track {
    LocalAudio(LocalAudioTrack),
    LocalVideo(LocalVideoTrack),
    RemoteAudio(RemoteAudioTrack),
    RemoteVideo(RemoteVideoTrack),
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

pub(super) use track_dispatch;

#[derive(Default)]
struct TrackEvents {
    pub muted: Option<Arc<dyn Fn()>>,
    pub unmuted: Option<Arc<dyn Fn()>>,
}

#[derive(Debug)]
struct TrackInfo {
    pub sid: TrackSid,
    pub name: String,
    pub kind: TrackKind,
    pub source: TrackSource,
    pub stream_state: StreamState,
    pub muted: bool,
    pub transceiver: Option<RtpTransceiver>,
}

pub(super) struct TrackInner {
    pub info: RwLock<TrackInfo>,
    pub rtc_track: MediaStreamTrack,
    pub events: RwLock<TrackEvents>,
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
            events: Default::default(),
        }
    }

    pub fn set_muted(&self, muted: bool) {
        let info = self.info.read();
        log::debug!("set_muted: {} {}", info.sid, muted);
        if info.muted == muted {
            return;
        }
        drop(info);

        if muted {
            self.rtc_track.set_enabled(false);
        } else {
            self.rtc_track.set_enabled(true);
        }

        self.info.write().muted = muted;

        if muted {
            if let Some(on_mute) = self.events.read().muted.clone() {
                on_mute();
            }
        } else {
            if let Some(on_unmute) = self.events.read().unmuted.clone() {
                on_unmute();
            }
        }
    }

    pub fn update_info(&self, new_info: proto::TrackInfo) {
        let mut info = self.info.write();
        info.name = new_info.name;
        info.sid = new_info.sid.into();
        info.kind =
            TrackKind::try_from(proto::TrackType::from_i32(new_info.r#type).unwrap()).unwrap();
        info.source = TrackSource::from(proto::TrackSource::from_i32(new_info.source).unwrap());
    }
}
