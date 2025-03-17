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

use super::StreamError;
use chrono::{DateTime, Utc};
use livekit_protocol::{data_stream as proto, enum_dispatch};
use std::collections::HashMap;

/// Information about a data stream.
pub trait StreamInfo {
    /// Unique identifier of the stream.
    fn id(&self) -> &str;

    /// Topic name used to route the stream to the appropriate handler.
    fn topic(&self) -> &str;

    /// When the stream was created.
    fn timestamp(&self) -> DateTime<Utc>;

    /// Total expected size in bytes (UTF-8 for text), if known.
    fn total_length(&self) -> Option<u64>;

    /// Additional attributes as needed for your application.
    fn attributes(&self) -> &HashMap<String, String>;

    /// MIME type.
    fn mime_type(&self) -> &str;
}

macro_rules! info_dispatch {
    ([$($variant:ident),+]) => {
        enum_dispatch!(
            [$($variant),+];
            fn id(self: &Self) -> &str;
            fn topic(self: &Self) -> &str;
            fn timestamp(self: &Self) -> DateTime<Utc>;
            fn total_length(self: &Self) -> Option<u64>;
            fn attributes(self: &Self) -> &HashMap<String, String>;
            fn mime_type(self: &Self) -> &str;
        );
    };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnyStreamInfo {
    Byte(ByteStreamInfo),
    Text(TextStreamInfo),
}

impl StreamInfo for AnyStreamInfo {
    info_dispatch!([Byte, Text]);
}

/// Information about a byte data stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ByteStreamInfo {
    base: BaseInfo,
    byte: ByteSpecificInfo,
}

/// Information about a text data stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextStreamInfo {
    base: BaseInfo,
    text: TextSpecificInfo,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaseInfo {
    id: String,
    topic: String,
    timestamp: DateTime<Utc>,
    total_length: Option<u64>,
    attributes: HashMap<String, String>,
    mime_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ByteSpecificInfo {
    name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TextSpecificInfo {
    operation_type: OperationType,
    version: i32,
    reply_to_stream_id: Option<String>,
    attached_stream_ids: Vec<String>,
    generated: bool,
}

/// Operation type for text streams.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OperationType {
    Create,
    Update,
    Delete,
    Reaction,
}

#[rustfmt::skip]
impl StreamInfo for ByteStreamInfo {
    fn id(&self) -> &str { &self.base.id }
    fn topic(&self) -> &str { &self.base.topic }
    fn timestamp(&self) -> DateTime<Utc> { self.base.timestamp }
    fn total_length(&self) -> Option<u64> { self.base.total_length }
    fn attributes(&self) -> &HashMap<String, String> { &self.base.attributes }
    fn mime_type(&self) -> &str { &self.base.mime_type }
}

impl ByteStreamInfo {
    pub fn name(&self) -> &str {
        &self.byte.name
    }
}

#[rustfmt::skip]
impl StreamInfo for TextStreamInfo {
    fn id(&self) -> &str { &self.base.id }
    fn topic(&self) -> &str { &self.base.topic }
    fn timestamp(&self) -> DateTime<Utc> { self.base.timestamp }
    fn total_length(&self) -> Option<u64> { self.base.total_length }
    fn attributes(&self) -> &HashMap<String, String> { &self.base.attributes }
    fn mime_type(&self) -> &str { &self.base.mime_type }
}

impl TextStreamInfo {
    pub fn operation_type(&self) -> OperationType {
        self.text.operation_type
    }
    pub fn version(&self) -> i32 {
        self.text.version
    }
    pub fn reply_to_stream_id(&self) -> Option<&String> {
        self.text.reply_to_stream_id.as_ref()
    }
    pub fn attached_stream_ids(&self) -> &[String] {
        &self.text.attached_stream_ids
    }
    pub fn generated(&self) -> bool {
        self.text.generated
    }
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
                Self::Byte(ByteStreamInfo { byte: byte_header.into(), base: header.into() })
            }
            proto::header::ContentHeader::TextHeader(text_header) => {
                Self::Text(TextStreamInfo { text: text_header.into(), base: header.into() })
            }
        };
        Ok(info)
    }
}

impl From<proto::Header> for BaseInfo {
    fn from(header: proto::Header) -> Self {
        BaseInfo {
            id: header.stream_id,
            topic: header.topic,
            timestamp: DateTime::<Utc>::from_timestamp_millis(header.timestamp)
                .unwrap_or_else(|| Utc::now()),
            total_length: header.total_length,
            attributes: header.attributes,
            mime_type: header.mime_type,
        }
    }
}

impl From<proto::ByteHeader> for ByteSpecificInfo {
    fn from(header: proto::ByteHeader) -> Self {
        ByteSpecificInfo { name: header.name }
    }
}

impl From<proto::TextHeader> for TextSpecificInfo {
    fn from(header: proto::TextHeader) -> Self {
        TextSpecificInfo {
            operation_type: header.operation_type().into(),
            version: header.version,
            reply_to_stream_id: (!header.reply_to_stream_id.is_empty())
                .then_some(header.reply_to_stream_id),
            attached_stream_ids: header.attached_stream_ids,
            generated: header.generated,
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

impl From<ByteStreamInfo> for AnyStreamInfo {
    /// Converts to enum variant [StreamInfo::Byte].
    fn from(info: ByteStreamInfo) -> Self {
        Self::Byte(info)
    }
}

impl From<TextStreamInfo> for AnyStreamInfo {
    /// Converts to enum variant [StreamInfo::Text].
    fn from(info: TextStreamInfo) -> Self {
        Self::Text(info)
    }
}
