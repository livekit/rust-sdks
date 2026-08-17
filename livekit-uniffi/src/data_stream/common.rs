// Copyright 2026 LiveKit, Inc.
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

//! Types that cross the FFI boundary for data streams: info/option records, enums, the error
//! wrapper, and the wire-packet decode helper.
//!
//! `Bytes` is already registered as a custom type by [`crate::data_track::common`]; it is reused
//! here rather than redefined (a second `custom_type!` in the same crate would conflict).
//! Participant identities cross as plain `String`.

use std::collections::HashMap;

use livekit_common as common;
use livekit_data_stream::{api as ds_api, backend as ds};
use livekit_protocol as proto;
use prost::Message;

// MARK: - Enums

/// Encryption applied to a data stream, mirroring [`common::EncryptionType`].
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncryptionType {
    None,
    Gcm,
    Custom,
}

impl From<common::EncryptionType> for EncryptionType {
    fn from(value: common::EncryptionType) -> Self {
        match value {
            common::EncryptionType::None => Self::None,
            common::EncryptionType::Gcm => Self::Gcm,
            common::EncryptionType::Custom => Self::Custom,
        }
    }
}

/// Operation type for text streams, mirroring [`ds_api::OperationType`].
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationType {
    Create,
    Update,
    Delete,
    Reaction,
}

impl From<ds_api::OperationType> for OperationType {
    fn from(value: ds_api::OperationType) -> Self {
        match value {
            ds_api::OperationType::Create => Self::Create,
            ds_api::OperationType::Update => Self::Update,
            ds_api::OperationType::Delete => Self::Delete,
            ds_api::OperationType::Reaction => Self::Reaction,
        }
    }
}

impl From<OperationType> for ds_api::OperationType {
    fn from(value: OperationType) -> Self {
        match value {
            OperationType::Create => Self::Create,
            OperationType::Update => Self::Update,
            OperationType::Delete => Self::Delete,
            OperationType::Reaction => Self::Reaction,
        }
    }
}

/// A capability a remote participant's client advertises, mirroring [`common::ClientCapability`].
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientCapability {
    Unused,
    PacketTrailer,
    CompressionDeflateRaw,
}

impl From<common::ClientCapability> for ClientCapability {
    fn from(value: common::ClientCapability) -> Self {
        match value {
            common::ClientCapability::Unused => Self::Unused,
            common::ClientCapability::PacketTrailer => Self::PacketTrailer,
            common::ClientCapability::CompressionDeflateRaw => Self::CompressionDeflateRaw,
            // `common::ClientCapability` is `#[non_exhaustive]`; treat anything newer as unusable.
            _ => Self::Unused,
        }
    }
}

impl From<ClientCapability> for common::ClientCapability {
    fn from(value: ClientCapability) -> Self {
        match value {
            ClientCapability::Unused => Self::Unused,
            ClientCapability::PacketTrailer => Self::PacketTrailer,
            ClientCapability::CompressionDeflateRaw => Self::CompressionDeflateRaw,
        }
    }
}

// MARK: - Info records

/// Information about a byte data stream. FFI wrapper around [`ds_api::ByteStreamInfo`].
#[derive(uniffi::Record, Clone, Debug)]
pub struct ByteStreamInfo {
    pub id: String,
    pub topic: String,
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: i64,
    pub total_length: Option<u64>,
    pub attributes: HashMap<String, String>,
    pub mime_type: String,
    pub name: String,
    pub encryption_type: EncryptionType,
}

impl From<ds_api::ByteStreamInfo> for ByteStreamInfo {
    fn from(info: ds_api::ByteStreamInfo) -> Self {
        let attributes = info.attributes();
        Self {
            id: info.id,
            topic: info.topic,
            timestamp_ms: info.timestamp.timestamp_millis(),
            total_length: info.total_length,
            attributes,
            mime_type: info.mime_type,
            name: info.name,
            encryption_type: info.encryption_type.into(),
        }
    }
}

/// Information about a text data stream. FFI wrapper around [`ds_api::TextStreamInfo`].
#[derive(uniffi::Record, Clone, Debug)]
pub struct TextStreamInfo {
    pub id: String,
    pub topic: String,
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: i64,
    pub total_length: Option<u64>,
    pub attributes: HashMap<String, String>,
    pub mime_type: String,
    pub operation_type: OperationType,
    pub version: i32,
    pub reply_to_stream_id: Option<String>,
    pub attached_stream_ids: Vec<String>,
    pub generated: bool,
    pub encryption_type: EncryptionType,
}

impl From<ds_api::TextStreamInfo> for TextStreamInfo {
    fn from(info: ds_api::TextStreamInfo) -> Self {
        let attributes = info.attributes();
        Self {
            id: info.id,
            topic: info.topic,
            timestamp_ms: info.timestamp.timestamp_millis(),
            total_length: info.total_length,
            attributes,
            mime_type: info.mime_type,
            operation_type: info.operation_type.into(),
            version: info.version,
            reply_to_stream_id: info.reply_to_stream_id,
            attached_stream_ids: info.attached_stream_ids,
            generated: info.generated,
            encryption_type: info.encryption_type.into(),
        }
    }
}

// MARK: - Option records

/// Options for sending a byte stream. FFI wrapper around [`ds_api::StreamByteOptions`].
#[derive(uniffi::Record, Clone, Debug, Default)]
pub struct StreamByteOptions {
    pub topic: String,
    pub attributes: HashMap<String, String>,
    #[uniffi(default = [])]
    pub destination_identities: Vec<String>,
    #[uniffi(default = None)]
    pub id: Option<String>,
    #[uniffi(default = None)]
    pub mime_type: Option<String>,
    #[uniffi(default = None)]
    pub name: Option<String>,
    #[uniffi(default = None)]
    pub total_length: Option<u64>,
    #[uniffi(default = None)]
    pub compress: Option<bool>,
    #[uniffi(default = None)]
    pub sender_identity: Option<String>,
}

impl From<StreamByteOptions> for ds_api::StreamByteOptions {
    fn from(options: StreamByteOptions) -> Self {
        Self {
            topic: options.topic,
            attributes: options.attributes,
            destination_identities: options
                .destination_identities
                .into_iter()
                .map(Into::into)
                .collect(),
            id: options.id,
            mime_type: options.mime_type,
            name: options.name,
            total_length: options.total_length,
            compress: options.compress,
            sender_identity: options.sender_identity.map(Into::into),
        }
    }
}

/// Options for sending a text stream. FFI wrapper around [`ds_api::StreamTextOptions`].
#[derive(uniffi::Record, Clone, Debug, Default)]
pub struct StreamTextOptions {
    pub topic: String,
    pub attributes: HashMap<String, String>,
    #[uniffi(default = [])]
    pub destination_identities: Vec<String>,
    #[uniffi(default = None)]
    pub id: Option<String>,
    #[uniffi(default = None)]
    pub operation_type: Option<OperationType>,
    #[uniffi(default = None)]
    pub version: Option<i32>,
    #[uniffi(default = None)]
    pub reply_to_stream_id: Option<String>,
    #[uniffi(default = [])]
    pub attached_stream_ids: Vec<String>,
    #[uniffi(default = None)]
    pub generated: Option<bool>,
    #[uniffi(default = None)]
    pub compress: Option<bool>,
    #[uniffi(default = None)]
    pub sender_identity: Option<String>,
}

impl From<StreamTextOptions> for ds_api::StreamTextOptions {
    fn from(options: StreamTextOptions) -> Self {
        Self {
            topic: options.topic,
            attributes: options.attributes,
            destination_identities: options
                .destination_identities
                .into_iter()
                .map(Into::into)
                .collect(),
            id: options.id,
            operation_type: options.operation_type.map(Into::into),
            version: options.version,
            reply_to_stream_id: options.reply_to_stream_id,
            attached_stream_ids: options.attached_stream_ids,
            generated: options.generated,
            compress: options.compress,
            sender_identity: options.sender_identity.map(Into::into),
        }
    }
}

// MARK: - Error

/// A data stream operation failed. Structured mirror of [`ds_api::StreamError`] so foreign callers
/// can map each case to their own error type; variants carrying a message forward it as `message`.
#[derive(uniffi::Error, thiserror::Error, Debug)]
pub enum DataStreamError {
    #[error("stream has already been closed")]
    AlreadyClosed,

    // Named `reason` rather than `message`: in Kotlin a variant field called `message` collides
    // with the `message` uniffi overrides from Throwable, and the collision cannot be renamed away
    // from uniffi.toml (renames of enum members declared in a submodule are silently dropped).
    #[error("stream closed abnormally: {reason}")]
    AbnormalEnd { reason: String },

    #[error("UTF-8 decoding error: {reason}")]
    Utf8 { reason: String },

    #[error("incoming header was invalid")]
    InvalidHeader,

    #[error("expected chunk index to be exactly one more than the previous")]
    MissedChunk,

    #[error("read length exceeded total length specified in stream header")]
    LengthExceeded,

    #[error("stream data is incomplete")]
    Incomplete,

    #[error("unable to send packet")]
    SendFailed,

    #[error("I/O error: {reason}")]
    Io { reason: String },

    #[error("internal error")]
    Internal,

    #[error("encryption type mismatch")]
    EncryptionTypeMismatch,

    #[error("stream header exceeds maximum size")]
    HeaderTooLarge,

    #[error("stream payload exceeds maximum size")]
    PayloadTooLarge,

    #[error("decompression failed")]
    Decompression,

    #[error("file name must be a plain file name without path separators or '..'")]
    InvalidFileName,
}

/// A foreign transport failed to deliver outbound packets; thrown by hosts from
/// [`OutgoingDataStreamManagerDelegate::on_packets_available`](super::outgoing::OutgoingDataStreamManagerDelegate::on_packets_available).
///
/// Morally `struct PacketDeliveryError(String)`, but uniffi error types must be enums, so the
/// string travels as the single variant's `reason` (free-form host context: logged, not parsed).
#[derive(uniffi::Error, thiserror::Error, Debug)]
pub enum PacketDeliveryError {
    #[error("failed to deliver packets: {reason}")]
    Failed { reason: String },
}

// Required because foreign code implements delegate methods returning this error: an exception
// that is NOT a `PacketDeliveryError` surfaces through this catch-all rather than aborting.
impl From<uniffi::UnexpectedUniFFICallbackError> for PacketDeliveryError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed { reason: error.reason }
    }
}

impl From<PacketDeliveryError> for DataStreamError {
    fn from(error: PacketDeliveryError) -> Self {
        log::error!("outbound packet delivery failed: {error}");
        Self::Internal
    }
}

impl From<ds_api::StreamError> for DataStreamError {
    fn from(error: ds_api::StreamError) -> Self {
        match error {
            ds_api::StreamError::AlreadyClosed => Self::AlreadyClosed,
            ds_api::StreamError::AbnormalEnd(reason) => Self::AbnormalEnd { reason },
            ds_api::StreamError::Utf8(error) => Self::Utf8 { reason: error.to_string() },
            ds_api::StreamError::InvalidHeader => Self::InvalidHeader,
            ds_api::StreamError::MissedChunk => Self::MissedChunk,
            ds_api::StreamError::LengthExceeded => Self::LengthExceeded,
            ds_api::StreamError::Incomplete => Self::Incomplete,
            ds_api::StreamError::SendFailed => Self::SendFailed,
            ds_api::StreamError::Io(error) => Self::Io { reason: error.to_string() },
            ds_api::StreamError::Internal => Self::Internal,
            ds_api::StreamError::EncryptionTypeMismatch => Self::EncryptionTypeMismatch,
            ds_api::StreamError::HeaderTooLarge => Self::HeaderTooLarge,
            ds_api::StreamError::PayloadTooLarge => Self::PayloadTooLarge,
            ds_api::StreamError::Decompression => Self::Decompression,
            ds_api::StreamError::InvalidFileName => Self::InvalidFileName,
        }
    }
}

// MARK: - Wire decode

/// Decodes a serialized [`proto::DataPacket`] carrying a data-stream header/chunk/trailer into an
/// incoming-manager input event. Returns `None` if the bytes don't decode or the packet isn't a
/// data-stream packet.
///
/// Encryption is defaulted to `None`: end-to-end encryption for data streams over this FFI is a
/// follow-up (the foreign side is expected to hand us already-decrypted packets).
pub(crate) fn decode_data_packet(bytes: &[u8]) -> Option<ds::incoming::PacketReceived> {
    let mut packet = proto::DataPacket::decode(bytes).ok()?;
    let identity: common::ParticipantIdentity = packet.participant_identity.clone().into();
    let ds_packet = match packet.value.take()? {
        proto::data_packet::Value::StreamHeader(header) => ds::Packet::Header {
            header: header.into(),
            encryption_type: common::EncryptionType::None,
        },
        proto::data_packet::Value::StreamChunk(chunk) => {
            ds::Packet::Chunk { chunk: chunk.into(), encryption_type: common::EncryptionType::None }
        }
        proto::data_packet::Value::StreamTrailer(trailer) => ds::Packet::Trailer(trailer.into()),
        _ => return None,
    };
    Some(ds::incoming::PacketReceived::new(ds_packet, identity))
}
