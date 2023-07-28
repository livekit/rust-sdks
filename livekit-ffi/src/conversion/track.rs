// Copyright 2023 LiveKit, Inc.
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

use crate::{proto, FfiHandleId};
use livekit::prelude::*;

impl From<TrackSource> for proto::TrackSource {
    fn from(source: TrackSource) -> proto::TrackSource {
        match source {
            TrackSource::Unknown => proto::TrackSource::SourceUnknown,
            TrackSource::Camera => proto::TrackSource::SourceCamera,
            TrackSource::Microphone => proto::TrackSource::SourceMicrophone,
            TrackSource::Screenshare => proto::TrackSource::SourceScreenshare,
            TrackSource::ScreenshareAudio => proto::TrackSource::SourceScreenshareAudio,
        }
    }
}

macro_rules! impl_publication_into {
    ($p:ty) => {
        impl From<$p> for proto::TrackPublicationInfo {
            fn from(p: $p) -> Self {
                Self {
                    name: p.name(),
                    sid: p.sid().to_string(),
                    kind: proto::TrackKind::from(p.kind()).into(),
                    source: proto::TrackSource::from(p.source()).into(),
                    width: p.dimension().0,
                    height: p.dimension().1,
                    mime_type: p.mime_type(),
                    simulcasted: p.simulcasted(),
                    muted: p.is_muted(),
                    remote: p.is_remote(),
                }
            }
        }
    };
}

impl_publication_into!(&LocalTrackPublication);
impl_publication_into!(&RemoteTrackPublication);
impl_publication_into!(&TrackPublication);

macro_rules! impl_track_into {
    ($fnc:ident, $t:ty) => {
        impl proto::TrackInfo {
            #[allow(dead_code)]
            pub fn $fnc(handle_id: FfiHandleId, track: $t) -> Self {
                Self {
                    handle: Some(handle_id.into()),
                    name: track.name(),
                    stream_state: proto::StreamState::from(track.stream_state()).into(),
                    sid: track.sid().to_string(),
                    kind: proto::TrackKind::from(track.kind()).into(),
                    muted: track.is_muted(),
                    remote: track.is_remote(),
                }
            }
        }
    };
}

impl_track_into!(from_local_audio_track, &LocalAudioTrack);
impl_track_into!(from_local_video_track, &LocalVideoTrack);
impl_track_into!(from_remote_audio_track, &RemoteAudioTrack);
impl_track_into!(from_remote_video_track, &RemoteVideoTrack);
impl_track_into!(from_track, &Track);
impl_track_into!(from_local_track, &LocalTrack);
impl_track_into!(from_remote_track, &RemoteTrack);

impl From<TrackKind> for proto::TrackKind {
    fn from(kind: TrackKind) -> Self {
        match kind {
            TrackKind::Audio => proto::TrackKind::KindAudio,
            TrackKind::Video => proto::TrackKind::KindVideo,
        }
    }
}

impl From<StreamState> for proto::StreamState {
    fn from(state: StreamState) -> Self {
        match state {
            StreamState::Active => Self::StateActive,
            StreamState::Paused => Self::StatePaused,
        }
    }
}

impl From<proto::TrackSource> for TrackSource {
    fn from(source: proto::TrackSource) -> Self {
        match source {
            proto::TrackSource::SourceUnknown => TrackSource::Unknown,
            proto::TrackSource::SourceCamera => TrackSource::Camera,
            proto::TrackSource::SourceMicrophone => TrackSource::Microphone,
            proto::TrackSource::SourceScreenshare => TrackSource::Screenshare,
            proto::TrackSource::SourceScreenshareAudio => TrackSource::ScreenshareAudio,
        }
    }
}
