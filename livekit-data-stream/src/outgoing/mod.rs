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
#[derive(Clone, Debug, Eq, PartialEq)]
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
    /// The identity the stream's packets are attributed to. If unspecified, the packets carry
    /// no explicit identity and the server attributes them to the sending participant. Only
    /// participants with the appropriate permission (e.g. agents) may impersonate another
    /// identity.
    pub sender_identity: Option<ParticipantIdentity>,
}

impl StreamByteOptions {
    pub fn new_with_topic(topic: impl Into<String>) -> Self {
        Self {
            topic: topic.into(),
            attributes: HashMap::new(),
            destination_identities: vec![],
            id: None,
            mime_type: None,
            name: None,
            total_length: None,
            compress: None,
            sender_identity: None,
        }
    }

    /// Sets the topic the stream is published to.
    pub fn with_topic(mut self, topic: String) -> Self {
        self.topic = topic;
        self
    }
    /// Replaces all attributes attached to the stream.
    pub fn with_attributes(mut self, attributes: HashMap<String, String>) -> Self {
        self.attributes = attributes;
        self
    }
    /// Adds a single attribute to the stream, overwriting any existing value for `key`.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
    /// Replaces the set of participant identities the stream is delivered to.
    /// An empty list delivers to all participants in the room.
    pub fn with_destination_identities(
        mut self,
        destination_identities: Vec<ParticipantIdentity>,
    ) -> Self {
        self.destination_identities = destination_identities;
        self
    }
    /// Adds a single participant identity to the stream's destinations.
    pub fn with_destination_identity(mut self, identity: impl Into<ParticipantIdentity>) -> Self {
        self.destination_identities.push(identity.into());
        self
    }
    /// Sets an explicit stream id. If unset, a new uuid is generated per call.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
    /// Sets the MIME type describing the stream's payload.
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }
    /// Sets a human-readable name for the stream (e.g. a file name).
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
    /// Sets the total byte length of the payload, when known ahead of time.
    pub fn with_total_length(mut self, total_length: u64) -> Self {
        self.total_length = Some(total_length);
        self
    }
    /// Sets whether to deflate-raw compress the payload when all recipients support it.
    /// Defaults to `true` (compression opt-out).
    pub fn with_compress(mut self, compress: bool) -> Self {
        self.compress = Some(compress);
        self
    }
    /// Sets the identity the stream's packets are attributed to. Only participants with the
    /// appropriate permission (e.g. agents) may impersonate another identity.
    pub fn with_sender_identity(mut self, identity: impl Into<ParticipantIdentity>) -> Self {
        self.sender_identity = Some(identity.into());
        self
    }
}

/// Options used when opening an outgoing text data stream.
#[derive(Clone, Debug, Eq, PartialEq)]
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
    /// The identity the stream's packets are attributed to. If unspecified, the packets carry
    /// no explicit identity and the server attributes them to the sending participant. Only
    /// participants with the appropriate permission (e.g. agents) may impersonate another
    /// identity.
    pub sender_identity: Option<ParticipantIdentity>,
}

impl StreamTextOptions {
    pub fn new_with_topic(topic: impl Into<String>) -> Self {
        Self {
            topic: topic.into(),
            attributes: HashMap::new(),
            destination_identities: vec![],
            id: None,
            operation_type: None,
            version: None,
            reply_to_stream_id: None,
            attached_stream_ids: vec![],
            generated: None,
            compress: None,
            sender_identity: None,
        }
    }

    /// Sets the topic the stream is published to.
    pub fn with_topic(mut self, topic: String) -> Self {
        self.topic = topic;
        self
    }
    /// Replaces all attributes attached to the stream.
    pub fn with_attributes(mut self, attributes: HashMap<String, String>) -> Self {
        self.attributes = attributes;
        self
    }
    /// Adds a single attribute to the stream, overwriting any existing value for `key`.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
    /// Replaces the set of participant identities the stream is delivered to.
    /// An empty list delivers to all participants in the room.
    pub fn with_destination_identities(
        mut self,
        destination_identities: Vec<ParticipantIdentity>,
    ) -> Self {
        self.destination_identities = destination_identities;
        self
    }
    /// Adds a single participant identity to the stream's destinations.
    pub fn with_destination_identity(mut self, identity: impl Into<ParticipantIdentity>) -> Self {
        self.destination_identities.push(identity.into());
        self
    }
    /// Sets an explicit stream id. If unset, a new uuid is generated per call.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
    /// Sets the operation this text stream represents (e.g. create or update).
    pub fn with_operation_type(mut self, operation_type: OperationType) -> Self {
        self.operation_type = Some(operation_type);
        self
    }
    /// Sets the version of the text, used to order updates to the same content.
    pub fn with_version(mut self, version: i32) -> Self {
        self.version = Some(version);
        self
    }
    /// Sets the id of the stream this text is a reply to.
    pub fn with_reply_to_stream_id(mut self, reply_to_stream_id: impl Into<String>) -> Self {
        self.reply_to_stream_id = Some(reply_to_stream_id.into());
        self
    }
    /// Replaces the set of stream ids attached to this text (e.g. referenced files).
    pub fn with_attached_stream_ids(mut self, attached_stream_ids: Vec<String>) -> Self {
        self.attached_stream_ids = attached_stream_ids;
        self
    }
    /// Adds a single attached stream id to this text.
    pub fn with_attached_stream_id(mut self, attached_stream_id: impl Into<String>) -> Self {
        self.attached_stream_ids.push(attached_stream_id.into());
        self
    }
    /// Sets whether the text was machine-generated (e.g. by an agent).
    pub fn with_generated(mut self, generated: bool) -> Self {
        self.generated = Some(generated);
        self
    }
    /// Sets whether to deflate-raw compress the payload when all recipients support it.
    /// Defaults to `true` (compression opt-out).
    pub fn with_compress(mut self, compress: bool) -> Self {
        self.compress = Some(compress);
        self
    }
    /// Sets the identity the stream's packets are attributed to. Only participants with the
    /// appropriate permission (e.g. agents) may impersonate another identity.
    pub fn with_sender_identity(mut self, identity: impl Into<ParticipantIdentity>) -> Self {
        self.sender_identity = Some(identity.into());
        self
    }
}
