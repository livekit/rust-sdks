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

use from_variants::FromVariants;
use livekit_common::ParticipantIdentity;

use crate::{
    incoming::AnyStreamReader,
    types::{Chunk, Packet, StreamId, Trailer},
};

pub struct PacketReceived {
    pub packet: Packet,
    pub participant_identity: ParticipantIdentity,
}

impl PacketReceived {
    pub fn new(packet: Packet, participant_identity: ParticipantIdentity) -> Self {
        Self { packet, participant_identity }
    }
}

/// An event fed into [`IncomingStreamManager::run`] by the host crate. Each corresponds to an
/// inbound data-stream packet (or a lifecycle signal) and carries everything the manager needs to
/// process it without reaching back into room state.
#[derive(FromVariants)]
pub enum InputEvent {
    PacketReceived(PacketReceived),
    /// Abort every open stream sent by this participant (they disconnected mid-send).
    AbortStreamsFrom(ParticipantIdentity),
    /// Abort every open stream (e.g. the local connection is going away). Unlike
    /// [`InputEvent::Shutdown`], the run loop keeps going so streams opened later are still handled.
    AbortAllStreams,
    /// Stop the run loop.
    Shutdown,
}

/// A new stream was opened; its reader should be delivered to the application (or routed
/// internally for reserved topics). Carries the sender's identity.
pub struct StreamOpened {
    pub stream_reader: AnyStreamReader,
    pub participant_identity: ParticipantIdentity,
}

/// A stream previously announced via [`StreamOpened`] has terminated and will produce no further
/// data: its trailer arrived, its inline payload completed, it failed with an error, or it was
/// aborted.
///
/// Emitted exactly once per opened stream. Hosts delivering streams on ordered topics use this to
/// know when a stream's handler can be considered finished on the wire (a trailer alone is not
/// enough: inline single-packet streams never receive one).
pub struct StreamClosed {
    pub stream_id: StreamId,
    pub participant_identity: ParticipantIdentity,
    /// Topic the stream was opened on.
    pub topic: String,
}

/// A "raw chunk received" notification, which is used to trigger
/// the deprecated [RoomEvent:::StreamChunkReceived] event.
pub struct ChunkReceived {
    pub chunk: Chunk,
    pub participant_identity: ParticipantIdentity,

    /// Topic of the stream this chunk belongs to, or `None` if the associated stream id could
    /// not be mapped to a topic.
    pub topic: Option<String>,
}

/// A "raw trailer received" notification, which is used to trigger
/// the deprecated [RoomEvent:::StreamTrailerReceived] event.
pub struct TrailerReceived {
    pub trailer: Trailer,
    pub participant_identity: ParticipantIdentity,

    /// Topic of the stream this chunk belongs to, or `None` if the associated stream id could
    /// not be mapped to a topic.
    ///
    /// See [`ChunkReceived::topic`].
    pub topic: Option<String>,
}

/// An event emitted by [`IncomingStreamManager::run`] for the host crate to surface. The manager
/// stays decoupled from `RoomEvent`; the host maps these onto its own event types.
#[derive(FromVariants)]
pub enum OutputEvent {
    StreamOpened(StreamOpened),
    StreamClosed(StreamClosed),
    ChunkReceived(ChunkReceived),
    TrailerReceived(TrailerReceived),
}
