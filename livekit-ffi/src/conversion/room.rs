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

use crate::{proto, server::room::FfiRoom};
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
    RoomInfo,
};

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

impl From<DisconnectReason> for proto::DisconnectReason {
    fn from(value: DisconnectReason) -> Self {
        match value {
            DisconnectReason::UnknownReason => Self::UnknownReason,
            DisconnectReason::ClientInitiated => Self::ClientInitiated,
            DisconnectReason::DuplicateIdentity => Self::DuplicateIdentity,
            DisconnectReason::ServerShutdown => Self::ServerShutdown,
            DisconnectReason::ParticipantRemoved => Self::ParticipantRemoved,
            DisconnectReason::RoomDeleted => Self::RoomDeleted,
            DisconnectReason::StateMismatch => Self::StateMismatch,
            DisconnectReason::JoinFailure => Self::JoinFailure,
            DisconnectReason::Migration => Self::Migration,
            DisconnectReason::SignalClose => Self::SignalClose,
            DisconnectReason::RoomClosed => Self::RoomClosed,
            DisconnectReason::UserUnavailable => Self::UserUnavailable,
            DisconnectReason::UserRejected => Self::UserRejected,
            DisconnectReason::SipTrunkFailure => Self::SipTrunkFailure,
            DisconnectReason::ConnectionTimeout => Self::ConnectionTimeout,
            DisconnectReason::MediaFailure => Self::MediaFailure,
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
        Self {
            urls: value.urls,
            username: value.username.unwrap_or_default(),
            password: value.password.unwrap_or_default(),
        }
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
            let provider_opts = opts.key_provider_options;

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

        let encryption = value.encryption.and_then(|opts| {
            let encryption_type = opts.encryption_type();
            let provider_opts = opts.key_provider_options;

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

        let mut options = RoomOptions::default();
        options.adaptive_stream = value.adaptive_stream.unwrap_or(options.adaptive_stream);
        options.auto_subscribe = value.auto_subscribe.unwrap_or(options.auto_subscribe);
        options.dynacast = value.dynacast.unwrap_or(options.dynacast);
        options.rtc_config = rtc_config;
        options.join_retries = value.join_retries.unwrap_or(options.join_retries);
        options.e2ee = e2ee;
        options.encryption = encryption;
        options
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
        let default_publish_options = TrackPublishOptions::default();
        let video_codec = opts.video_codec.map(|x| proto::VideoCodec::try_from(x).ok()).flatten();
        let source = opts.source.map(|x| proto::TrackSource::try_from(x).ok()).flatten();

        Self {
            video_codec: video_codec.map(Into::into).unwrap_or(default_publish_options.video_codec),
            source: source.map(Into::into).unwrap_or(default_publish_options.source),
            video_encoding: opts
                .video_encoding
                .map(Into::into)
                .or(default_publish_options.video_encoding),
            audio_encoding: opts
                .audio_encoding
                .map(Into::into)
                .or(default_publish_options.audio_encoding),
            dtx: opts.dtx.unwrap_or(default_publish_options.dtx),
            red: opts.red.unwrap_or(default_publish_options.red),
            simulcast: opts.simulcast.unwrap_or(default_publish_options.simulcast),
            stream: opts.stream.unwrap_or(default_publish_options.stream),
            preconnect_buffer: opts
                .preconnect_buffer
                .unwrap_or(default_publish_options.preconnect_buffer),
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
        proto::RoomInfo {
            sid: room.maybe_sid().map(|x| x.to_string()),
            name: room.name(),
            metadata: room.metadata(),
            lossy_dc_buffered_amount_low_threshold: room
                .data_channel_options(DataPacketKind::Lossy)
                .buffered_amount_low_threshold,
            reliable_dc_buffered_amount_low_threshold: room
                .data_channel_options(DataPacketKind::Reliable)
                .buffered_amount_low_threshold,
            empty_timeout: room.empty_timeout(),
            departure_timeout: room.departure_timeout(),
            max_participants: room.max_participants(),
            creation_time: room.creation_time(),
            num_participants: room.num_participants(),
            num_publishers: room.num_publishers(),
            active_recording: room.active_recording(),
        }
    }
}

impl From<RoomInfo> for proto::RoomInfo {
    fn from(room: RoomInfo) -> Self {
        proto::RoomInfo {
            sid: room.sid.map(|x| x.to_string()),
            name: room.name,
            metadata: room.metadata,
            lossy_dc_buffered_amount_low_threshold: room
                .lossy_dc_options
                .buffered_amount_low_threshold,
            reliable_dc_buffered_amount_low_threshold: room
                .reliable_dc_options
                .buffered_amount_low_threshold,
            empty_timeout: room.empty_timeout,
            departure_timeout: room.departure_timeout,
            max_participants: room.max_participants,
            creation_time: room.creation_time,
            num_participants: room.num_participants,
            num_publishers: room.num_publishers,
            active_recording: room.active_recording,
        }
    }
}

impl From<proto::ChatMessage> for livekit::ChatMessage {
    fn from(proto_msg: proto::ChatMessage) -> Self {
        livekit::ChatMessage {
            id: proto_msg.id,
            message: proto_msg.message,
            timestamp: proto_msg.timestamp,
            edit_timestamp: proto_msg.edit_timestamp,
            deleted: proto_msg.deleted,
            generated: proto_msg.generated,
        }
    }
}

impl From<livekit::ChatMessage> for proto::ChatMessage {
    fn from(msg: livekit::ChatMessage) -> Self {
        proto::ChatMessage {
            id: msg.id,
            message: msg.message,
            timestamp: msg.timestamp,
            edit_timestamp: msg.edit_timestamp,
            deleted: msg.deleted.into(),
            generated: msg.generated.into(),
        }
    }
}

impl From<livekit_protocol::data_stream::Header> for proto::data_stream::Header {
    fn from(msg: livekit_protocol::data_stream::Header) -> Self {
        let content_header = match msg.content_header {
            Some(livekit_protocol::data_stream::header::ContentHeader::TextHeader(text_header)) => {
                Some(proto::data_stream::header::ContentHeader::TextHeader(
                    proto::data_stream::TextHeader {
                        operation_type: text_header.operation_type,
                        version: Some(text_header.version),
                        reply_to_stream_id: Some(text_header.reply_to_stream_id),
                        attached_stream_ids: text_header.attached_stream_ids,
                        generated: Some(text_header.generated),
                    },
                ))
            }
            Some(livekit_protocol::data_stream::header::ContentHeader::ByteHeader(byte_header)) => {
                Some(proto::data_stream::header::ContentHeader::ByteHeader(
                    proto::data_stream::ByteHeader { name: byte_header.name },
                ))
            }
            None => None,
        };

        proto::data_stream::Header {
            stream_id: msg.stream_id,
            timestamp: msg.timestamp,
            topic: msg.topic,
            mime_type: msg.mime_type,
            total_length: msg.total_length,
            attributes: msg.attributes,
            content_header,
        }
    }
}

impl From<proto::data_stream::Header> for livekit_protocol::data_stream::Header {
    fn from(msg: proto::data_stream::Header) -> Self {
        let content_header = match msg.content_header {
            Some(proto::data_stream::header::ContentHeader::TextHeader(text_header)) => {
                Some(livekit_protocol::data_stream::header::ContentHeader::TextHeader(
                    livekit_protocol::data_stream::TextHeader {
                        operation_type: text_header.operation_type,
                        version: text_header.version.unwrap_or_default(),
                        reply_to_stream_id: text_header.reply_to_stream_id.unwrap_or_default(),
                        attached_stream_ids: text_header.attached_stream_ids,
                        generated: text_header.generated.unwrap_or(false),
                    },
                ))
            }
            Some(proto::data_stream::header::ContentHeader::ByteHeader(byte_header)) => {
                Some(livekit_protocol::data_stream::header::ContentHeader::ByteHeader(
                    livekit_protocol::data_stream::ByteHeader { name: byte_header.name },
                ))
            }
            None => None,
        };

        livekit_protocol::data_stream::Header {
            stream_id: msg.stream_id,
            timestamp: msg.timestamp,
            topic: msg.topic,
            mime_type: msg.mime_type,
            total_length: msg.total_length,
            attributes: msg.attributes,
            content_header,
            encryption_type: 0,
        }
    }
}

impl From<livekit_protocol::data_stream::Chunk> for proto::data_stream::Chunk {
    fn from(msg: livekit_protocol::data_stream::Chunk) -> Self {
        proto::data_stream::Chunk {
            stream_id: msg.stream_id,
            content: msg.content,
            chunk_index: msg.chunk_index,
            version: Some(msg.version),
            iv: msg.iv,
        }
    }
}

impl From<proto::data_stream::Chunk> for livekit_protocol::data_stream::Chunk {
    fn from(msg: proto::data_stream::Chunk) -> Self {
        livekit_protocol::data_stream::Chunk {
            stream_id: msg.stream_id,
            content: msg.content,
            chunk_index: msg.chunk_index,
            version: msg.version.unwrap_or(0),
            iv: msg.iv,
        }
    }
}

impl From<livekit_protocol::data_stream::Trailer> for proto::data_stream::Trailer {
    fn from(msg: livekit_protocol::data_stream::Trailer) -> Self {
        Self { stream_id: msg.stream_id, reason: msg.reason, attributes: msg.attributes }
    }
}

impl From<proto::data_stream::Trailer> for livekit_protocol::data_stream::Trailer {
    fn from(msg: proto::data_stream::Trailer) -> Self {
        Self { stream_id: msg.stream_id, reason: msg.reason, attributes: msg.attributes }
    }
}
