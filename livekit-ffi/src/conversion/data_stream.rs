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

use crate::proto;
use livekit::{ByteStreamInfo, OperationType, TextStreamInfo};

impl From<TextStreamInfo> for proto::TextStreamInfo {
    fn from(info: TextStreamInfo) -> Self {
        Self {
            stream_id: info.id,
            timestamp: info.timestamp.timestamp_millis(),
            mime_type: info.mime_type,
            topic: info.topic,
            total_length: info.total_length,
            attributes: info.attributes,
            operation_type: proto::text_stream_info::OperationType::from(info.operation_type)
                .into(),
            version: Some(info.version),
            reply_to_stream_id: info.reply_to_stream_id,
            attached_stream_ids: info.attached_stream_ids,
            generated: Some(info.generated),
        }
    }
}

impl From<ByteStreamInfo> for proto::ByteStreamInfo {
    fn from(info: ByteStreamInfo) -> Self {
        Self {
            stream_id: info.id,
            timestamp: info.timestamp.timestamp_millis(),
            mime_type: info.mime_type,
            topic: info.topic,
            total_length: info.total_length,
            attributes: info.attributes,
            name: info.name,
        }
    }
}

impl From<OperationType> for proto::text_stream_info::OperationType {
    fn from(value: OperationType) -> Self {
        match value {
            OperationType::Create => Self::Create,
            OperationType::Update => Self::Update,
            OperationType::Delete => Self::Delete,
            OperationType::Reaction => Self::Reaction,
        }
    }
}
