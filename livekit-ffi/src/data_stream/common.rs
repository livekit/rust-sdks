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

//! Types that cross the FFI boundary for data streams: info/option records, enums and the error
//! wrapper.
//!
//! `Bytes` is already registered as a remote type by [`crate::common`]; it is reused
//! here rather than redefined (a second `custom_type!` in the same crate would conflict).
//! Participant identities cross as plain `String`.

use std::{collections::HashMap, sync::Arc};

use livekit_common as common;
use livekit_data_stream::api as ds_api;

use super::outgoing::StreamProgressDelegate;

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

impl From<EncryptionType> for common::EncryptionType {
    fn from(value: EncryptionType) -> Self {
        match value {
            EncryptionType::None => Self::None,
            EncryptionType::Gcm => Self::Gcm,
            EncryptionType::Custom => Self::Custom,
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

// MARK: - Progress

/// Progress of a data stream. FFI wrapper around [`ds_api::StreamProgress`].
#[derive(uniffi::Record, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamProgress {
    /// Bytes received or sent so far, counted before compression.
    pub bytes_processed: u64,
    /// Total bytes expected, or `None` for a stream of unknown length.
    pub bytes_total: Option<u64>,
}

impl From<ds_api::StreamProgress> for StreamProgress {
    fn from(progress: ds_api::StreamProgress) -> Self {
        Self { bytes_processed: progress.bytes_processed(), bytes_total: progress.bytes_total() }
    }
}

// MARK: - Option records

/// Converts to [`ds_api::SendByteOptions`] for `send_bytes` and `send_file`, which drop
/// `total_length`, and to [`ds_api::StreamByteOptions`] for `stream_bytes`, which drops `compress`
/// and `progress`.
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
    /// Optional send progress delegate; ignored by incremental `stream_bytes`.
    #[uniffi(default = None)]
    pub progress: Option<Arc<dyn StreamProgressDelegate>>,
}

/// Converts to send options, keeping `compress`; the send derives the payload length.
impl From<StreamByteOptions> for ds_api::SendByteOptions {
    fn from(value: StreamByteOptions) -> Self {
        let mut options = Self::new_with_topic(value.topic);
        options.attributes = value.attributes;
        options.destination_identities =
            value.destination_identities.into_iter().map(Into::into).collect();
        options.id = value.id;
        options.sender_identity = value.sender_identity.map(Into::into);
        options.mime_type = value.mime_type;
        options.name = value.name;
        options.compress = value.compress;
        options.on_progress = progress_callback(value.progress);
        options
    }
}

/// Converts to incremental stream options, keeping `total_length` and ignoring `compress`
/// and `progress`.
impl From<StreamByteOptions> for ds_api::StreamByteOptions {
    fn from(value: StreamByteOptions) -> Self {
        let mut options = Self::new_with_topic(value.topic);
        options.attributes = value.attributes;
        options.destination_identities =
            value.destination_identities.into_iter().map(Into::into).collect();
        options.id = value.id;
        options.sender_identity = value.sender_identity.map(Into::into);
        options.mime_type = value.mime_type;
        options.name = value.name;
        options.total_length = value.total_length;
        options
    }
}

/// Converts to [`ds_api::SendTextOptions`] for `send_text`, which drops `total_length`, and to
/// [`ds_api::StreamTextOptions`] for `stream_text`, which drops `compress` and `progress`.
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
    /// Total byte length of the text, when known ahead of time. Used by `stream_text`;
    /// `send_text` sets it from the text it sends.
    #[uniffi(default = None)]
    pub total_length: Option<u64>,
    /// Optional send progress delegate; ignored by incremental `stream_text`.
    #[uniffi(default = None)]
    pub progress: Option<Arc<dyn StreamProgressDelegate>>,
}

/// Converts to send options, keeping `compress`; the send derives the payload length.
impl From<StreamTextOptions> for ds_api::SendTextOptions {
    fn from(value: StreamTextOptions) -> Self {
        let mut options = Self::new_with_topic(value.topic);
        options.attributes = value.attributes;
        options.destination_identities =
            value.destination_identities.into_iter().map(Into::into).collect();
        options.id = value.id;
        options.sender_identity = value.sender_identity.map(Into::into);
        options.operation_type = value.operation_type.map(Into::into);
        options.version = value.version;
        options.reply_to_stream_id = value.reply_to_stream_id;
        options.attached_stream_ids = value.attached_stream_ids;
        options.generated = value.generated;
        options.compress = value.compress;
        options.on_progress = progress_callback(value.progress);
        options
    }
}

/// Converts to incremental stream options, keeping `total_length` and ignoring `compress`
/// and `progress`.
impl From<StreamTextOptions> for ds_api::StreamTextOptions {
    fn from(value: StreamTextOptions) -> Self {
        let mut options = Self::new_with_topic(value.topic);
        options.attributes = value.attributes;
        options.destination_identities =
            value.destination_identities.into_iter().map(Into::into).collect();
        options.id = value.id;
        options.sender_identity = value.sender_identity.map(Into::into);
        options.operation_type = value.operation_type.map(Into::into);
        options.version = value.version;
        options.reply_to_stream_id = value.reply_to_stream_id;
        options.attached_stream_ids = value.attached_stream_ids;
        options.generated = value.generated;
        options.total_length = value.total_length;
        options
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

    #[error("encryption type mismatch: expected {expected:?}, received {received:?}")]
    EncryptionTypeMismatch { expected: EncryptionType, received: EncryptionType },

    #[error("stream header exceeds maximum size")]
    HeaderTooLarge,

    #[error("stream payload exceeds maximum size")]
    PayloadTooLarge,

    #[error("decompression failed")]
    Decompression,

    #[error("file name must be a plain file name without path separators or '..'")]
    InvalidFileName,
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
            ds_api::StreamError::EncryptionTypeMismatch { expected, received } => {
                Self::EncryptionTypeMismatch {
                    expected: expected.into(),
                    received: received.into(),
                }
            }
            ds_api::StreamError::HeaderTooLarge => Self::HeaderTooLarge,
            ds_api::StreamError::PayloadTooLarge => Self::PayloadTooLarge,
            ds_api::StreamError::Decompression => Self::Decompression,
            ds_api::StreamError::InvalidFileName => Self::InvalidFileName,
        }
    }
}

/// Wraps the foreign progress delegate for the core's send options.
fn progress_callback(
    progress: Option<Arc<dyn StreamProgressDelegate>>,
) -> Option<ds_api::StreamProgressCallback> {
    progress.map(|progress| {
        ds_api::StreamProgressCallback::new(move |p| progress.on_progress(p.into()))
    })
}
