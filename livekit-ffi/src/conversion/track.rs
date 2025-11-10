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

use livekit::{participant::ParticipantTrackPermission, prelude::*};

use crate::{
    proto,
    server::room::{FfiPublication, FfiTrack},
};

impl From<&FfiPublication> for proto::TrackPublicationInfo {
    fn from(value: &FfiPublication) -> Self {
        let publication = &value.publication;
        Self {
            name: publication.name(),
            sid: publication.sid().to_string(),
            kind: proto::TrackKind::from(publication.kind()).into(),
            source: proto::TrackSource::from(publication.source()).into(),
            width: publication.dimension().0,
            height: publication.dimension().1,
            mime_type: publication.mime_type(),
            simulcasted: publication.simulcasted(),
            muted: publication.is_muted(),
            remote: publication.is_remote(),
            encryption_type: proto::EncryptionType::from(publication.encryption_type()).into(),
            audio_features: publication
                .audio_features()
                .into_iter()
                .map(|i| proto::AudioTrackFeature::from(i).into())
                .collect(),
        }
    }
}

impl From<&FfiTrack> for proto::TrackInfo {
    fn from(value: &FfiTrack) -> Self {
        let track = &value.track;
        Self {
            name: track.name(),
            stream_state: proto::StreamState::from(track.stream_state()).into(),
            sid: track.sid().to_string(),
            kind: proto::TrackKind::from(track.kind()).into(),
            muted: track.is_muted(),
            remote: track.is_remote(),
        }
    }
}

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

impl From<ParticipantTrackPermission> for proto::ParticipantTrackPermission {
    fn from(value: ParticipantTrackPermission) -> Self {
        proto::ParticipantTrackPermission {
            participant_identity: value.participant_identity.to_string(),
            allow_all: Some(value.allow_all),
            allowed_track_sids: value
                .allowed_track_sids
                .into_iter()
                .map(|sid| sid.to_string())
                .collect(),
        }
    }
}

impl From<proto::ParticipantTrackPermission> for ParticipantTrackPermission {
    fn from(value: proto::ParticipantTrackPermission) -> Self {
        Self {
            participant_identity: value.participant_identity.into(),
            allow_all: value.allow_all.unwrap_or(false),
            allowed_track_sids: value
                .allowed_track_sids
                .into_iter()
                .map(|sid| sid.try_into().unwrap())
                .collect(),
        }
    }
}

impl From<proto::AudioTrackFeature> for AudioTrackFeature {
    fn from(value: proto::AudioTrackFeature) -> Self {
        match value {
            proto::AudioTrackFeature::TfStereo => AudioTrackFeature::TfStereo,
            proto::AudioTrackFeature::TfNoDtx => AudioTrackFeature::TfNoDtx,
            proto::AudioTrackFeature::TfAutoGainControl => AudioTrackFeature::TfAutoGainControl,
            proto::AudioTrackFeature::TfEchoCancellation => AudioTrackFeature::TfEchoCancellation,
            proto::AudioTrackFeature::TfNoiseSuppression => AudioTrackFeature::TfNoiseSuppression,
            proto::AudioTrackFeature::TfEnhancedNoiseCancellation => {
                AudioTrackFeature::TfEnhancedNoiseCancellation
            }
            proto::AudioTrackFeature::TfPreconnectBuffer => AudioTrackFeature::TfPreconnectBuffer,
        }
    }
}

impl From<AudioTrackFeature> for proto::AudioTrackFeature {
    fn from(value: AudioTrackFeature) -> Self {
        match value {
            AudioTrackFeature::TfStereo => proto::AudioTrackFeature::TfStereo,
            AudioTrackFeature::TfNoDtx => proto::AudioTrackFeature::TfNoDtx,
            AudioTrackFeature::TfAutoGainControl => proto::AudioTrackFeature::TfAutoGainControl,
            AudioTrackFeature::TfEchoCancellation => proto::AudioTrackFeature::TfEchoCancellation,
            AudioTrackFeature::TfNoiseSuppression => proto::AudioTrackFeature::TfNoiseSuppression,
            AudioTrackFeature::TfEnhancedNoiseCancellation => {
                proto::AudioTrackFeature::TfEnhancedNoiseCancellation
            }
            AudioTrackFeature::TfPreconnectBuffer => proto::AudioTrackFeature::TfPreconnectBuffer,
        }
    }
}
