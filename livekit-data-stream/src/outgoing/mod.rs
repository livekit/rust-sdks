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

//! Outgoing data streams: the [`manager::Manager`] and the writers it produces.

use livekit_common::ParticipantIdentity;
use std::collections::HashMap;

use crate::types::OperationType;

pub(crate) mod manager;

mod constants;
mod raw_stream;
mod stream_writer;

pub use stream_writer::{ByteStreamWriter, StreamWriter, TextStreamWriter};

/// Options used when opening an outgoing byte data stream.
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct StreamByteOptions {
    pub topic: String,
    pub attributes: HashMap<String, String>,
    pub destination_identities: Vec<ParticipantIdentity>,
    /// The id associated with the stream. If unspecified, a new uuid will be created and used per
    /// call.
    pub id: Option<String>,
    pub mime_type: Option<String>,
    pub name: Option<String>,
    pub total_length: Option<u64>,
    /// Whether to deflate-raw compress the payload when all recipients support it.
    /// Defaults to `true` (compression opt-out). Ignored by the incremental `stream_bytes`.
    pub compress: Option<bool>,
}

/// Options used when opening an outgoing text data stream.
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct StreamTextOptions {
    pub topic: String,
    pub attributes: HashMap<String, String>,
    pub destination_identities: Vec<ParticipantIdentity>,
    /// The id associated with the stream. If unspecified, a new uuid will be created and used per
    /// call.
    pub id: Option<String>,
    pub operation_type: Option<OperationType>,
    pub version: Option<i32>,
    pub reply_to_stream_id: Option<String>,
    pub attached_stream_ids: Vec<String>,
    pub generated: Option<bool>,
    /// Whether to deflate-raw compress the payload when all recipients support it.
    /// Defaults to `true` (compression opt-out). Ignored by the incremental `stream_text`.
    pub compress: Option<bool>,
}
