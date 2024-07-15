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

use livekit::{
    e2ee::{
        key_provider::{KeyProvider, KeyProviderOptions},
        E2eeOptions, EncryptionType,
    },
    options::{AudioEncoding, TrackPublishOptions, VideoEncoding},
    prelude::*,
    webrtc::{
        native::frame_cryptor::EncryptionState,
        prelude::{ContinualGatheringPolicy, IceServer, IceTransportsType, RtcConfiguration},
    },
};

use crate::{proto, server::room::FfiRoom};

impl From<EncryptionState> for proto::EncryptionState {
    fn from(value: EncryptionState) -> Self {
        match value {
            EncryptionState::New => Self::New,
            EncryptionState::Ok => Self::Ok,
            EncryptionState::EncryptionFailed => Self::EncryptionFailed,
            EncryptionState::DecryptionFailed => Self::DecryptionFailed,
            EncryptionState::MissingKey => Self::MissingKey,
            EncryptionState::KeyRatcheted => Self::KeyRatcheted,
            EncryptionState::InternalError => Self::InternalError,
        }
    }
}

impl From<ConnectionQuality> for proto::ConnectionQuality {
    fn from(value: ConnectionQuality) -> Self {
        match value {
            ConnectionQuality::Excellent => Self::QualityExcellent,
            ConnectionQuality::Good => Self::QualityGood,
            ConnectionQuality::Poor => Self::QualityPoor,
            ConnectionQuality::Lost => Self::QualityLost,
        }
    }
}

impl From<ConnectionState> for proto::ConnectionState {
    fn from(value: ConnectionState) -> Self {
        match value {
            ConnectionState::Connected => Self::ConnConnected,
            ConnectionState::Reconnecting => Self::ConnReconnecting,
            ConnectionState::Disconnected => Self::ConnDisconnected,
        }
    }
}

impl From<proto::EncryptionType> for EncryptionType {
    fn from(value: proto::EncryptionType) -> Self {
        match value {
            proto::EncryptionType::None => Self::None,
            proto::EncryptionType::Gcm => Self::Gcm,
            proto::EncryptionType::Custom => Self::Custom,
        }
    }
}

impl From<EncryptionType> for proto::EncryptionType {
    fn from(value: EncryptionType) -> Self {
        match value {
            EncryptionType::None => Self::None,
            EncryptionType::Gcm => Self::Gcm,
            EncryptionType::Custom => Self::Custom,
        }
    }
}

impl From<proto::KeyProviderOptions> for KeyProviderOptions {
    fn from(value: proto::KeyProviderOptions) -> Self {
        Self {
            ratchet_window_size: value.ratchet_window_size,
            ratchet_salt: value.ratchet_salt,
            failure_tolerance: value.failure_tolerance,
        }
    }
}

impl From<proto::IceTransportType> for IceTransportsType {
    fn from(value: proto::IceTransportType) -> Self {
        match value {
            proto::IceTransportType::TransportRelay => Self::Relay,
            proto::IceTransportType::TransportNohost => Self::NoHost,
            proto::IceTransportType::TransportAll => Self::All,
        }
    }
}

impl From<proto::ContinualGatheringPolicy> for ContinualGatheringPolicy {
    fn from(value: proto::ContinualGatheringPolicy) -> Self {
        match value {
            proto::ContinualGatheringPolicy::GatherOnce => Self::GatherOnce,
            proto::ContinualGatheringPolicy::GatherContinually => Self::GatherContinually,
        }
    }
}

impl From<proto::IceServer> for IceServer {
    fn from(value: proto::IceServer) -> Self {
        Self { urls: value.urls, username: value.username, password: value.password }
    }
}

impl From<proto::RtcConfig> for RtcConfiguration {
    fn from(value: proto::RtcConfig) -> Self {
        let default = RoomOptions::default().rtc_config; // Always use RoomOptions as the default reference

        Self {
            ice_transport_type: value.ice_transport_type.map_or(default.ice_transport_type, |x| {
                proto::IceTransportType::try_from(x).unwrap().into()
            }),
            continual_gathering_policy: value
                .continual_gathering_policy
                .map_or(default.continual_gathering_policy, |x| {
                    proto::ContinualGatheringPolicy::try_from(x).unwrap().into()
                }),
            ice_servers: value.ice_servers.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<proto::RoomOptions> for RoomOptions {
    fn from(value: proto::RoomOptions) -> Self {
        let e2ee = value.e2ee.and_then(|opts| {
            let encryption_type = opts.encryption_type();
            let Some(provider_opts) = opts.key_provider_options else {
                return None;
            };

            Some(E2eeOptions {
                encryption_type: encryption_type.into(),
                key_provider: if provider_opts.shared_key.is_some() {
                    let shared_key = provider_opts.shared_key.clone().unwrap();
                    KeyProvider::with_shared_key(provider_opts.into(), shared_key)
                } else {
                    KeyProvider::new(provider_opts.into())
                },
            })
        });

        let rtc_config =
            value.rtc_config.map(Into::into).unwrap_or(RoomOptions::default().rtc_config);

        Self {
            adaptive_stream: value.adaptive_stream,
            auto_subscribe: value.auto_subscribe,
            dynacast: value.dynacast,
            e2ee,
            rtc_config,
            join_retries: value.join_retries,
        }
    }
}

impl From<proto::DataPacketKind> for DataPacketKind {
    fn from(value: proto::DataPacketKind) -> Self {
        match value {
            proto::DataPacketKind::KindReliable => Self::Reliable,
            proto::DataPacketKind::KindLossy => Self::Lossy,
        }
    }
}

impl From<DataPacketKind> for proto::DataPacketKind {
    fn from(value: DataPacketKind) -> Self {
        match value {
            DataPacketKind::Reliable => Self::KindReliable,
            DataPacketKind::Lossy => Self::KindLossy,
        }
    }
}

impl From<proto::TrackPublishOptions> for TrackPublishOptions {
    fn from(opts: proto::TrackPublishOptions) -> Self {
        Self {
            video_codec: opts.video_codec().into(),
            source: opts.source().into(),
            video_encoding: opts.video_encoding.map(Into::into),
            audio_encoding: opts.audio_encoding.map(Into::into),
            dtx: opts.dtx,
            red: opts.red,
            simulcast: opts.simulcast,
        }
    }
}

impl From<proto::VideoEncoding> for VideoEncoding {
    fn from(opts: proto::VideoEncoding) -> Self {
        Self { max_bitrate: opts.max_bitrate, max_framerate: opts.max_framerate }
    }
}

impl From<proto::AudioEncoding> for AudioEncoding {
    fn from(opts: proto::AudioEncoding) -> Self {
        Self { max_bitrate: opts.max_bitrate }
    }
}

impl From<&FfiRoom> for proto::RoomInfo {
    fn from(value: &FfiRoom) -> Self {
        let room = &value.inner.room;
        Self {
            sid: room.maybe_sid().map(|x| x.to_string()),
            name: room.name(),
            metadata: room.metadata(),
        }
    }
}
