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

use chrono::{DateTime, Utc};
use livekit_common::EncryptionType;
use std::collections::HashMap;

use crate::types::{ByteHeader, ContentHeader, Header, OperationType, TextHeader};
use crate::utils::StreamError;

/// Information about a byte data stream.
#[derive(Clone, Debug)]
pub struct ByteStreamInfo {
    /// Unique identifier of the stream.
    pub id: String,
    /// Topic name used to route the stream to the appropriate handler.
    pub topic: String,
    /// When the stream was created.
    pub timestamp: DateTime<Utc>,
    /// Total expected size in bytes, if known.
    pub total_length: Option<u64>,
    /// Additional attributes as needed for your application.
    pub attributes: HashMap<String, String>,
    /// The MIME type of the stream data.
    pub mime_type: String,
    /// The name of the file being sent.
    pub name: String,
    /// The encryption used
    pub encryption_type: EncryptionType,
    /// Test-only: expose whether the byte stream was compressed or not.
    #[cfg(feature = "__e2e-test")]
    pub is_compressed: bool,
    /// Test-only: expose whether the byte stream was sent inline on the header packet
    #[cfg(feature = "__e2e-test")]
    pub is_inline: bool,
}

/// Information about a text data stream.
#[derive(Clone, Debug)]
pub struct TextStreamInfo {
    /// Unique identifier of the stream.
    pub id: String,
    /// Topic name used to route the stream to the appropriate handler.
    pub topic: String,
    /// When the stream was created.
    pub timestamp: DateTime<Utc>,
    /// Total expected size in bytes, if known.
    pub total_length: Option<u64>,
    /// Additional attributes as needed for your application.
    pub attributes: HashMap<String, String>,
    /// The MIME type of the stream data.
    pub mime_type: String,
    pub operation_type: OperationType,
    pub version: i32,
    pub reply_to_stream_id: Option<String>,
    pub attached_stream_ids: Vec<String>,
    pub generated: bool,
    /// The encryption used
    pub encryption_type: EncryptionType,
    /// Test-only: expose whether the byte stream was compressed or not.
    #[cfg(feature = "__e2e-test")]
    pub is_compressed: bool,
    /// Test-only: expose whether the byte stream was sent inline on the header packet
    #[cfg(feature = "__e2e-test")]
    pub is_inline: bool,
}

// MARK: - Type conversion

impl TryFrom<Header> for AnyStreamInfo {
    type Error = StreamError;

    fn try_from(header: Header) -> Result<Self, Self::Error> {
        Self::try_from_with_encryption(header, EncryptionType::None)
    }
}

impl AnyStreamInfo {
    pub fn try_from_with_encryption(
        mut header: Header,
        encryption_type: EncryptionType,
    ) -> Result<Self, StreamError> {
        let Some(content_header) = header.content_header.take() else {
            Err(StreamError::InvalidHeader)?
        };
        let info = match content_header {
            ContentHeader::ByteHeader(byte_header) => Self::Byte(
                ByteStreamInfo::from_headers_with_encryption(header, byte_header, encryption_type),
            ),
            ContentHeader::TextHeader(text_header) => Self::Text(
                TextStreamInfo::from_headers_with_encryption(header, text_header, encryption_type),
            ),
        };
        Ok(info)
    }
}

impl ByteStreamInfo {
    pub(crate) fn from_headers(header: Header, byte_header: ByteHeader) -> Self {
        Self::from_headers_with_encryption(header, byte_header, EncryptionType::None)
    }

    pub(crate) fn from_headers_with_encryption(
        header: Header,
        byte_header: ByteHeader,
        encryption_type: EncryptionType,
    ) -> Self {
        Self {
            id: header.stream_id.to_string(),
            topic: header.topic,
            timestamp: DateTime::<Utc>::from_timestamp_millis(header.timestamp)
                .unwrap_or_else(|| Utc::now()),
            total_length: header.total_length,
            attributes: header.attributes,
            mime_type: header.mime_type,
            name: byte_header.name,
            encryption_type,
            #[cfg(feature = "__e2e-test")]
            is_compressed: header.compression != crate::types::CompressionType::None,
            #[cfg(feature = "__e2e-test")]
            is_inline: header.inline_content.is_some_and(|c| !c.is_empty()),
        }
    }
}

impl TextStreamInfo {
    pub(crate) fn from_headers(header: Header, text_header: TextHeader) -> Self {
        Self::from_headers_with_encryption(header, text_header, EncryptionType::None)
    }

    pub(crate) fn from_headers_with_encryption(
        header: Header,
        text_header: TextHeader,
        encryption_type: EncryptionType,
    ) -> Self {
        Self {
            id: header.stream_id.to_string(),
            topic: header.topic,
            timestamp: DateTime::<Utc>::from_timestamp_millis(header.timestamp)
                .unwrap_or_else(|| Utc::now()),
            total_length: header.total_length,
            attributes: header.attributes,
            mime_type: header.mime_type,
            operation_type: text_header.operation_type,
            version: text_header.version,
            reply_to_stream_id: text_header.reply_to_stream_id.map(|stream_id| stream_id.into()),
            attached_stream_ids: text_header
                .attached_stream_ids
                .into_iter()
                .map(Into::into)
                .collect(),
            generated: text_header.generated,
            encryption_type,
            #[cfg(feature = "__e2e-test")]
            is_compressed: header.compression != crate::types::CompressionType::None,
            #[cfg(feature = "__e2e-test")]
            is_inline: header.inline_content.is_some_and(|c| !c.is_empty()),
        }
    }
}

// MARK: - Dispatch

#[derive(Clone, Debug)]
pub(crate) enum AnyStreamInfo {
    Byte(ByteStreamInfo),
    Text(TextStreamInfo),
}

impl AnyStreamInfo {
    livekit_common::enum_dispatch!(
        [Byte, Text];
        pub fn id(self: &Self) -> &str;
        pub fn total_length(self: &Self) -> Option<u64>;
        pub fn encryption_type(self: &Self) -> EncryptionType;
    );
}

#[rustfmt::skip]
macro_rules! stream_info {
    () => {
        pub(crate) fn id(&self) -> &str { &self.id }
        pub(crate) fn total_length(&self) -> Option<u64> { self.total_length }
        pub(crate) fn encryption_type(&self) -> EncryptionType { self.encryption_type }
    };
}

impl ByteStreamInfo {
    stream_info!();
}

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
