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
use livekit_protocol::data_stream as proto;
use std::collections::HashMap;

/// Information about a byte data stream.
#[derive(Clone, Debug)]
#[non_exhaustive]
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
#[non_exhaustive]
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
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
#[non_exhaustive]
pub enum OperationType {
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
        let timestamp =
            DateTime::<Utc>::from_timestamp_millis(header.timestamp).unwrap_or_else(|| Utc::now());
        let info = match content_header {
            proto::header::ContentHeader::ByteHeader(byte_header) => Self::Byte(ByteStreamInfo {
                id: header.stream_id,
                topic: header.topic,
                timestamp,
                total_length: header.total_length,
                attributes: header.attributes,
                mime_type: header.mime_type,
                name: byte_header.name,
            }),
            proto::header::ContentHeader::TextHeader(text_header) => Self::Text(TextStreamInfo {
                id: header.stream_id,
                topic: header.topic,
                timestamp,
                total_length: header.total_length,
                attributes: header.attributes,
                mime_type: header.mime_type,
                operation_type: text_header.operation_type().into(),
                version: text_header.version,
                reply_to_stream_id: (!text_header.reply_to_stream_id.is_empty())
                    .then_some(text_header.reply_to_stream_id),
                attached_stream_ids: text_header.attached_stream_ids,
                generated: text_header.generated,
            }),
        };
        Ok(info)
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
pub(super) enum AnyStreamInfo {
    Byte(ByteStreamInfo),
    Text(TextStreamInfo),
}

impl AnyStreamInfo {
    pub(super) fn id(&self) -> &str {
        match self {
            AnyStreamInfo::Byte(info) => &info.id,
            AnyStreamInfo::Text(info) => &info.id,
        }
    }
    pub(super) fn total_length(&self) -> Option<u64> {
        match self {
            AnyStreamInfo::Byte(info) => info.total_length,
            AnyStreamInfo::Text(info) => info.total_length,
        }
    }
}
