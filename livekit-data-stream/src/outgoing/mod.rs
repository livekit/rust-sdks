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
use std::{collections::HashMap, fmt, sync::Arc};

use crate::{types::OperationType, utils::StreamProgress};

pub(crate) mod manager;

mod constants;
mod raw_stream;
mod stream_writer;

pub use stream_writer::{ByteStreamWriter, StreamWriter, TextStreamWriter};

/// Receives outgoing send progress on the thread polling the send; callbacks should return quickly.
///
/// Clones share the callback, and equality compares callback identity rather than behavior.
#[derive(Clone)]
pub struct StreamProgressCallback(Arc<dyn Fn(StreamProgress) + Send + Sync>);

impl StreamProgressCallback {
    /// Wraps a callback that receives progress before compression.
    pub fn new(callback: impl Fn(StreamProgress) + Send + Sync + 'static) -> Self {
        Self(Arc::new(callback))
    }

    /// Invokes the callback with the current progress.
    pub fn call(&self, progress: StreamProgress) {
        (self.0)(progress);
    }
}

impl fmt::Debug for StreamProgressCallback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StreamProgressCallback")
    }
}

impl PartialEq for StreamProgressCallback {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for StreamProgressCallback {}

macro_rules! common_setters {
    () => {
        /// Sets the topic the stream is published to, accepting any value convertible to `String`.
        pub fn with_topic(mut self, topic: impl Into<String>) -> Self {
            self.topic = topic.into();
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
        pub fn with_destination_identity(
            mut self,
            identity: impl Into<ParticipantIdentity>,
        ) -> Self {
            self.destination_identities.push(identity.into());
            self
        }
        /// Sets an explicit stream id. If unset, a new uuid is generated per call.
        pub fn with_id(mut self, id: impl Into<String>) -> Self {
            self.id = Some(id.into());
            self
        }
        /// Sets the identity the stream's packets are attributed to. Only participants with the
        /// appropriate permission (e.g. agents) may impersonate another identity.
        pub fn with_sender_identity(mut self, identity: impl Into<ParticipantIdentity>) -> Self {
            self.sender_identity = Some(identity.into());
            self
        }
    };
}

macro_rules! text_setters {
    () => {
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
    };
}

macro_rules! byte_setters {
    () => {
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
    };
}

macro_rules! send_setters {
    () => {
        /// Sets whether to deflate-raw compress when all recipients support it.
        /// Defaults to `true` (compression opt-out).
        pub fn with_compress(mut self, compress: bool) -> Self {
            self.compress = Some(compress);
            self
        }
        /// Sets the progress callback. One-shot text and byte sends report once after the
        /// transport acknowledges the complete send. Files report as chunks are sent.
        pub fn with_on_progress(
            mut self,
            callback: impl Fn(StreamProgress) + Send + Sync + 'static,
        ) -> Self {
            self.on_progress = Some(StreamProgressCallback::new(callback));
            self
        }
    };
}

macro_rules! stream_setters {
    () => {
        /// Sets the total byte length of the payload, when known ahead of time.
        pub fn with_total_length(mut self, total_length: u64) -> Self {
            self.total_length = Some(total_length);
            self
        }
    };
}

/// Options for [`send_text`](manager::Manager::send_text). Progress is reported once after the complete send is acknowledged.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SendTextOptions {
    /// Topic the stream is published to.
    pub topic: String,
    /// Attributes attached to the stream.
    pub attributes: HashMap<String, String>,
    /// Recipients; an empty list delivers to all participants.
    pub destination_identities: Vec<ParticipantIdentity>,
    /// Explicit stream id, or a new UUID per call when unspecified.
    pub id: Option<String>,
    /// Operation this text stream represents.
    pub operation_type: Option<OperationType>,
    /// Version used to order updates to the same content.
    pub version: Option<i32>,
    /// Id of the stream this text replies to.
    pub reply_to_stream_id: Option<String>,
    /// Ids of streams attached to this text.
    pub attached_stream_ids: Vec<String>,
    /// Whether the text was machine-generated.
    pub generated: Option<bool>,
    /// Identity attributed to the packets, or the sending participant when unspecified.
    /// Impersonating another identity requires the appropriate permission (e.g. agents).
    pub sender_identity: Option<ParticipantIdentity>,
    /// Whether to deflate-raw compress when all recipients support it. Defaults to true.
    pub compress: Option<bool>,
    /// Progress callback, counting payload bytes before compression.
    pub on_progress: Option<StreamProgressCallback>,
}

impl SendTextOptions {
    /// Creates options for the given topic. Use the `with_*` setters to customize them.
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
            sender_identity: None,
            compress: None,
            on_progress: None,
        }
    }

    common_setters!();
    text_setters!();
    send_setters!();
}

/// Options for [`send_bytes`](manager::Manager::send_bytes) and [`send_file`](manager::Manager::send_file).
///
/// Byte sends report progress once after completion. Files report after each chunk or compressed
/// encoder drain, counting input bytes which can run ahead of chunks sent. The whole file is
/// reported only after its last chunk was sent.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SendByteOptions {
    /// Topic the stream is published to.
    pub topic: String,
    /// Attributes attached to the stream.
    pub attributes: HashMap<String, String>,
    /// Recipients; an empty list delivers to all participants.
    pub destination_identities: Vec<ParticipantIdentity>,
    /// Explicit stream id, or a new UUID per call when unspecified.
    pub id: Option<String>,
    /// MIME type describing the payload.
    pub mime_type: Option<String>,
    /// Human-readable name; `send_file` defaults to the file name.
    pub name: Option<String>,
    /// Identity attributed to the packets, or the sending participant when unspecified.
    /// Impersonating another identity requires the appropriate permission (e.g. agents).
    pub sender_identity: Option<ParticipantIdentity>,
    /// Whether to deflate-raw compress when all recipients support it. Defaults to true.
    pub compress: Option<bool>,
    /// Progress callback, counting payload bytes before compression.
    pub on_progress: Option<StreamProgressCallback>,
}

impl SendByteOptions {
    /// Creates options for the given topic. Use the `with_*` setters to customize them.
    pub fn new_with_topic(topic: impl Into<String>) -> Self {
        Self {
            topic: topic.into(),
            attributes: HashMap::new(),
            destination_identities: vec![],
            id: None,
            mime_type: None,
            name: None,
            sender_identity: None,
            compress: None,
            on_progress: None,
        }
    }

    common_setters!();
    byte_setters!();
    send_setters!();
}

/// Options for [`stream_text`](manager::Manager::stream_text), which never compresses or inlines content.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamTextOptions {
    /// Topic the stream is published to.
    pub topic: String,
    /// Attributes attached to the stream.
    pub attributes: HashMap<String, String>,
    /// Recipients; an empty list delivers to all participants.
    pub destination_identities: Vec<ParticipantIdentity>,
    /// Explicit stream id, or a new UUID per call when unspecified.
    pub id: Option<String>,
    /// Operation this text stream represents.
    pub operation_type: Option<OperationType>,
    /// Version used to order updates to the same content.
    pub version: Option<i32>,
    /// Id of the stream this text replies to.
    pub reply_to_stream_id: Option<String>,
    /// Ids of streams attached to this text.
    pub attached_stream_ids: Vec<String>,
    /// Whether the text was machine-generated.
    pub generated: Option<bool>,
    /// Identity attributed to the packets, or the sending participant when unspecified.
    /// Impersonating another identity requires the appropriate permission (e.g. agents).
    pub sender_identity: Option<ParticipantIdentity>,
    /// Declared total byte length, when known. The receiver fails the stream on a mismatch.
    pub total_length: Option<u64>,
}

impl StreamTextOptions {
    /// Creates options for the given topic. Use the `with_*` setters to customize them.
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
            sender_identity: None,
            total_length: None,
        }
    }

    common_setters!();
    text_setters!();
    stream_setters!();
}

/// Options for [`stream_bytes`](manager::Manager::stream_bytes), which never compresses or inlines content.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamByteOptions {
    /// Topic the stream is published to.
    pub topic: String,
    /// Attributes attached to the stream.
    pub attributes: HashMap<String, String>,
    /// Recipients; an empty list delivers to all participants.
    pub destination_identities: Vec<ParticipantIdentity>,
    /// Explicit stream id, or a new UUID per call when unspecified.
    pub id: Option<String>,
    /// MIME type describing the payload.
    pub mime_type: Option<String>,
    /// Human-readable name for the stream.
    pub name: Option<String>,
    /// Identity attributed to the packets, or the sending participant when unspecified.
    /// Impersonating another identity requires the appropriate permission (e.g. agents).
    pub sender_identity: Option<ParticipantIdentity>,
    /// Declared total byte length, when known. The receiver fails the stream on a mismatch.
    pub total_length: Option<u64>,
}

impl StreamByteOptions {
    /// Creates options for the given topic. Use the `with_*` setters to customize them.
    pub fn new_with_topic(topic: impl Into<String>) -> Self {
        Self {
            topic: topic.into(),
            attributes: HashMap::new(),
            destination_identities: vec![],
            id: None,
            mime_type: None,
            name: None,
            sender_identity: None,
            total_length: None,
        }
    }

    common_setters!();
    byte_setters!();
    stream_setters!();
}
