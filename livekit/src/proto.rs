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

use livekit_protocol::*;

use crate::{e2ee::EncryptionType, participant, track, DataPacketKind};

// Conversions
impl From<ConnectionQuality> for participant::ConnectionQuality {
    fn from(value: ConnectionQuality) -> Self {
        match value {
            ConnectionQuality::Excellent => Self::Excellent,
            ConnectionQuality::Good => Self::Good,
            ConnectionQuality::Poor => Self::Poor,
            ConnectionQuality::Lost => Self::Lost,
        }
    }
}

impl TryFrom<TrackType> for track::TrackKind {
    type Error = &'static str;

    fn try_from(r#type: TrackType) -> Result<Self, Self::Error> {
        match r#type {
            TrackType::Audio => Ok(Self::Audio),
            TrackType::Video => Ok(Self::Video),
            TrackType::Data => Err("data tracks are not implemented yet"),
        }
    }
}

impl From<track::TrackKind> for TrackType {
    fn from(kind: track::TrackKind) -> Self {
        match kind {
            track::TrackKind::Audio => Self::Audio,
            track::TrackKind::Video => Self::Video,
        }
    }
}

impl From<TrackSource> for track::TrackSource {
    fn from(source: TrackSource) -> Self {
        match source {
            TrackSource::Camera => Self::Camera,
            TrackSource::Microphone => Self::Microphone,
            TrackSource::ScreenShare => Self::Screenshare,
            TrackSource::ScreenShareAudio => Self::ScreenshareAudio,
            TrackSource::Unknown => Self::Unknown,
        }
    }
}

impl From<track::TrackSource> for TrackSource {
    fn from(source: track::TrackSource) -> Self {
        match source {
            track::TrackSource::Camera => Self::Camera,
            track::TrackSource::Microphone => Self::Microphone,
            track::TrackSource::Screenshare => Self::ScreenShare,
            track::TrackSource::ScreenshareAudio => Self::ScreenShareAudio,
            track::TrackSource::Unknown => Self::Unknown,
        }
    }
}

impl From<DataPacketKind> for data_packet::Kind {
    fn from(kind: DataPacketKind) -> Self {
        match kind {
            DataPacketKind::Lossy => Self::Lossy,
            DataPacketKind::Reliable => Self::Reliable,
        }
    }
}

impl From<data_packet::Kind> for DataPacketKind {
    fn from(kind: data_packet::Kind) -> Self {
        match kind {
            data_packet::Kind::Lossy => Self::Lossy,
            data_packet::Kind::Reliable => Self::Reliable,
        }
    }
}

impl From<encryption::Type> for EncryptionType {
    fn from(value: livekit_protocol::encryption::Type) -> Self {
        match value {
            livekit_protocol::encryption::Type::None => Self::None,
            livekit_protocol::encryption::Type::Gcm => Self::Gcm,
            livekit_protocol::encryption::Type::Custom => Self::Custom,
        }
    }
}

impl From<EncryptionType> for encryption::Type {
    fn from(value: EncryptionType) -> Self {
        match value {
            EncryptionType::None => Self::None,
            EncryptionType::Gcm => Self::Gcm,
            EncryptionType::Custom => Self::Custom,
        }
    }
}
