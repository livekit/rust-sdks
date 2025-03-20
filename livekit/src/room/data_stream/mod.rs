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

use thiserror::Error;
use chrono::{DateTime, Utc};
use livekit_protocol::{data_stream as proto, enum_dispatch};
use std::collections::HashMap;

mod incoming;
mod outgoing;

pub use incoming::*;
pub use outgoing::*;

/// Result type for data stream operations.
pub type StreamResult<T> = Result<T, StreamError>;

/// Error type for data stream operations.
#[derive(Debug, Error)]
pub enum StreamError {
    #[error("stream with this ID is already opened")]
    AlreadyOpened,

    #[error("stream has already been closed")]
    AlreadyClosed,

    #[error("stream closed abnormally: {0}")]
    AbnormalEnd(String),

    #[error("UTF-8 decoding error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("incoming header was invalid")]
    InvalidHeader,

    #[error("expected chunk index to be exactly one more than the previous")]
    MissedChunk,

    #[error("read length exceeded total length specified in stream header")]
    LengthExceeded,

    #[error("stream data is incomplete")]
    Incomplete,

    #[error("stream terminated before completion")]
    Terminated,

    #[error("cannot perform operations on unknown stream")]
    UnknownStream,

    #[error("handler already registered for this stream type")]
    HandlerAlreadyRegistered,

    #[error("no handler registered for the incoming stream")]
    HandlerNotRegistered,

    #[error("unable to send packet")]
    SendFailed,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("internal error")]
    Internal
}

/// Progress of a data stream.
#[derive(Clone, Copy, Default, Debug, Hash, Eq, PartialEq)]
pub struct StreamProgress {
    chunk_index: u64,
    pub bytes_processed: u64,
    pub bytes_total: Option<u64>
}

impl StreamProgress {
    fn percentage(&self) -> Option<f32> {
        self.bytes_total
            .map(|total| self.bytes_processed as f32 / total as f32)
    }
}

/// Information about a byte data stream.
#[derive(Clone, Debug)]
pub struct ByteStreamInfo {
    pub id: String,
    pub topic: String,
    pub timestamp: DateTime<Utc>,
    pub total_length: Option<u64>,
    pub attributes: HashMap<String, String>,
    pub mime_type: String,
    pub name: String,
}

/// Information about a text data stream.
#[derive(Clone, Debug)]
pub struct TextStreamInfo {
    pub id: String,
    pub topic: String,
    pub timestamp: DateTime<Utc>,
    pub total_length: Option<u64>,
    pub attributes: HashMap<String, String>,
    pub mime_type: String,
    pub operation_type: OperationType,
    pub version: i32,
    pub reply_to_stream_id: Option<String>,
    pub attached_stream_ids: Vec<String>,
    pub generated: bool,
}

/// Operation type for text streams.
#[derive(Clone, Copy, Default, Debug, Hash, Eq, PartialEq)]
pub enum OperationType {
    #[default]
    Create,
    Update,
    Delete,
    Reaction,
}

// MARK: - Protocol type conversion

impl TryFrom<proto::Header> for AnyStreamInfo {
    type Error = StreamError;

    fn try_from(mut header: proto::Header) -> Result<Self, Self::Error> {
        let Some(content_header) = header.content_header.take() else {
            Err(StreamError::InvalidHeader)?
        };
        let info = match content_header {
            proto::header::ContentHeader::ByteHeader(byte_header) => {
                Self::Byte(ByteStreamInfo::from_headers(header, byte_header))
            }
            proto::header::ContentHeader::TextHeader(text_header) => {
                Self::Text(TextStreamInfo::from_headers(header, text_header))
            }
        };
        Ok(info)
    }
}

impl ByteStreamInfo {
    pub(crate) fn from_headers(header: proto::Header, byte_header: proto::ByteHeader) -> Self {
        Self {
            id: header.stream_id,
            topic: header.topic,
            timestamp: DateTime::<Utc>::from_timestamp_millis(header.timestamp)
                .unwrap_or_else(|| Utc::now()),
            total_length: header.total_length,
            attributes: header.attributes,
            mime_type: header.mime_type,
            name: byte_header.name,
        }
    }
}

impl TextStreamInfo {
    pub(crate) fn from_headers(header: proto::Header, text_header: proto::TextHeader) -> Self {
        Self {
            id: header.stream_id,
            topic: header.topic,
            timestamp: DateTime::<Utc>::from_timestamp_millis(header.timestamp)
                .unwrap_or_else(|| Utc::now()),
            total_length: header.total_length,
            attributes: header.attributes,
            mime_type: header.mime_type,
            operation_type: text_header.operation_type().into(),
            version: text_header.version,
            reply_to_stream_id: (!text_header.reply_to_stream_id.is_empty())
                .then_some(text_header.reply_to_stream_id),
            attached_stream_ids: text_header.attached_stream_ids,
            generated: text_header.generated,
        }
    }
}

impl From<proto::OperationType> for OperationType {
    fn from(op_type: proto::OperationType) -> Self {
        match op_type {
            proto::OperationType::Create => OperationType::Create,
            proto::OperationType::Update => OperationType::Update,
            proto::OperationType::Delete => OperationType::Delete,
            proto::OperationType::Reaction => OperationType::Reaction,
        }
    }
}
// MARK: - Dispatch

#[derive(Clone, Debug)]
pub enum AnyStreamInfo {
    Byte(ByteStreamInfo),
    Text(TextStreamInfo),
}

impl AnyStreamInfo {
    enum_dispatch!(
        [Byte, Text];
        pub fn id(self: &Self) -> &str;
        pub fn topic(self: &Self) -> &str;
        pub fn timestamp(self: &Self) -> &DateTime<Utc>;
        pub fn total_length(self: &Self) -> Option<u64>;
        pub fn attributes(self: &Self) -> &HashMap<String, String>;
        pub fn mime_type(self: &Self) -> &str;
    );
}

#[rustfmt::skip]
macro_rules! stream_info {
    () => {
        pub fn id(&self) -> &str { &self.id }
        pub fn topic(&self) -> &str { &self.topic }
        pub fn timestamp(&self) -> &DateTime<Utc> { &self.timestamp }
        pub fn total_length(&self) -> Option<u64> { self.total_length }
        pub fn attributes(&self) -> &HashMap<String, String> { &self.attributes }
        pub fn mime_type(&self) -> &str { &self.mime_type }
    };
}

#[rustfmt::skip]
impl ByteStreamInfo {
    stream_info!();
}

#[rustfmt::skip]
impl TextStreamInfo {
    stream_info!();
}

impl From<ByteStreamInfo> for AnyStreamInfo {
    fn from(info: ByteStreamInfo) -> Self {
        Self::Byte(info)
    }
}

impl From<TextStreamInfo> for AnyStreamInfo {
    fn from(info: TextStreamInfo) -> Self {
        Self::Text(info)
    }
}
