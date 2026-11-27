// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{fmt::Debug, sync::Arc};

use crate::observation::{Observed, StateRevision, StateSnapshot};
use libwebrtc::enum_dispatch;
use libwebrtc::{prelude::*, stats::RtcStats};
use livekit_protocol::{self as proto};
use parking_lot::Mutex;
use thiserror::Error;

use crate::prelude::*;

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
    #[error("could not find published track with sid: {0:?}")]
    TrackNotFound(TrackSid),
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

/// Video quality for simulcasted tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VideoQuality {
    Low,
    Medium,
    High,
}

impl From<VideoQuality> for proto::VideoQuality {
    fn from(quality: VideoQuality) -> Self {
        match quality {
            VideoQuality::Low => Self::Low,
            VideoQuality::Medium => Self::Medium,
            VideoQuality::High => Self::High,
        }
    }
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
            pub fn is_enabled(self: &Self) -> bool;
            pub fn enable(self: &Self) -> ();
            pub fn disable(self: &Self) -> ();
            pub fn is_muted(self: &Self) -> bool;
            pub fn is_remote(self: &Self) -> bool;
            pub fn on_muted(self: &Self, on_mute: impl Fn(Track) + Send + 'static) -> ();
            pub fn on_unmuted(self: &Self, on_unmute: impl Fn(Track) + Send + 'static) -> ();

            pub(crate) fn transceiver(self: &Self) -> Option<RtpTransceiver>;
            pub(crate) fn set_transceiver(self: &Self, transceiver: Option<RtpTransceiver>) -> ();
            pub(crate) fn update_info(self: &Self, info: proto::TrackInfo) -> ();
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

    fn observation_inner(&self) -> &TrackInner {
        match self {
            Self::LocalAudio(t) => &t.inner,
            Self::LocalVideo(t) => &t.inner,
            Self::RemoteAudio(t) => &t.inner,
            Self::RemoteVideo(t) => &t.inner,
        }
    }

    /// Core-owned values only. Enabled/codec parameters remain Rust-mediated live native queries.
    pub fn observed_state(&self) -> StateSnapshot<TrackValues> {
        let inner = self.observation_inner();
        let info = inner.info.read();
        inner.revision.snapshot(TrackValues {
            sid: info.sid.clone(),
            name: info.name.clone(),
            kind: info.kind,
            source: info.source,
            stream_state: info.stream_state,
            muted: info.muted,
            audio_features: info.audio_features.clone(),
        })
    }

    /// Waits independently for core-owned state; reconnect does not reset its revision.
    pub async fn state_did_change(&self, since: Option<u64>) -> Option<StateSnapshot<TrackValues>> {
        if self.observation_inner().revision.wait(since).await {
            Some(self.observed_state())
        } else {
            None
        }
    }

    pub(crate) fn same_instance(&self, other: &Self) -> bool {
        std::ptr::eq(self.observation_inner(), other.observation_inner())
    }

    pub(crate) fn finish_observation(&self) {
        // Local tracks can be republished in another room and retain their lifetime/revision.
        if self.is_remote() {
            let inner = self.observation_inner();
            let info = inner.info.read();
            let changed = inner.revision.mark_finished();
            drop(info);
            if changed {
                inner.revision.notify();
            }
        }
    }

    pub fn rtc_track(&self) -> MediaStreamTrack {
        match self {
            Self::LocalAudio(track) => track.rtc_track().into(),
            Self::LocalVideo(track) => track.rtc_track().into(),
            Self::RemoteAudio(track) => track.rtc_track().into(),
            Self::RemoteVideo(track) => track.rtc_track().into(),
        }
    }

    pub async fn get_stats(&self) -> RoomResult<Vec<RtcStats>> {
        match self {
            Self::LocalAudio(track) => track.get_stats().await,
            Self::LocalVideo(track) => track.get_stats().await,
            Self::RemoteAudio(track) => track.get_stats().await,
            Self::RemoteVideo(track) => track.get_stats().await,
        }
    }

    /// Returns the codec clock rate from the receiver's RTP parameters.
    /// This is the actual sample rate of the audio delivered by WebRTC's decoder.
    pub fn codec_clock_rate(&self) -> Option<u32> {
        self.transceiver()?
            .receiver()
            .parameters()
            .codecs
            .first()
            .and_then(|c| c.clock_rate)
            .map(|r| r as u32)
    }
}

pub(super) use track_dispatch;

type MutedHandler = Box<dyn Fn(Track) + Send>;
type UnmutedHandler = Box<dyn Fn(Track) + Send>;

#[derive(Default)]
struct TrackEvents {
    // These mute handlers are only called for local tracks
    pub muted: Option<MutedHandler>,
    pub unmuted: Option<UnmutedHandler>,
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
    pub audio_features: Vec<proto::AudioTrackFeature>,
}

pub(super) struct TrackInner {
    revision: Arc<StateRevision>,
    info: Observed<TrackInfo>,
    rtc_track: MediaStreamTrack,
    events: Mutex<TrackEvents>,
}

pub(super) fn new_inner(
    sid: TrackSid,
    name: String,
    kind: TrackKind,
    rtc_track: MediaStreamTrack,
) -> TrackInner {
    let revision = Arc::new(StateRevision::default());
    TrackInner {
        info: Observed::new(
            TrackInfo {
                sid,
                name,
                kind,
                source: TrackSource::Unknown,
                stream_state: StreamState::Active,
                muted: false,
                transceiver: None,
                audio_features: Vec::new(),
            },
            &revision,
        ),
        revision,
        rtc_track,
        events: Default::default(),
    }
}

/// This is only called for local tracks
pub(super) fn set_muted(inner: &Arc<TrackInner>, track: &Track, muted: bool) {
    let info = inner.info.read();
    if info.muted == muted {
        return;
    }
    drop(info);

    if muted {
        inner.rtc_track.set_enabled(false);
    } else {
        inner.rtc_track.set_enabled(true);
    }

    {
        let mut info = inner.info.write();
        if inner.revision.is_terminal() {
            return;
        }
        info.muted = muted;
    }

    if muted {
        if let Some(on_mute) = inner.events.lock().muted.as_ref() {
            on_mute(track.clone());
        }
    } else if let Some(on_unmute) = inner.events.lock().unmuted.as_ref() {
        on_unmute(track.clone());
    }
}

pub(super) fn update_info(inner: &Arc<TrackInner>, _track: &Track, new_info: proto::TrackInfo) {
    let mut info = inner.info.write();
    if inner.revision.is_terminal() {
        return;
    }
    info.kind = TrackKind::try_from(new_info.r#type()).unwrap();
    info.source = TrackSource::from(new_info.source());
    info.name = new_info.name.clone();
    info.sid = new_info.sid.clone().try_into().unwrap();
    info.audio_features =
        new_info.audio_features().into_iter().map(|item| item.try_into().unwrap()).collect();
}

/// Immutable core track values; native media state is deliberately queried separately.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackValues {
    pub sid: TrackSid,
    pub name: String,
    pub kind: TrackKind,
    pub source: TrackSource,
    pub stream_state: StreamState,
    pub muted: bool,
    pub audio_features: Vec<proto::AudioTrackFeature>,
}
