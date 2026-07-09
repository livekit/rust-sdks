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

use crate::incoming::AnyStreamReader;
use crate::types::{Chunk, Packet, Trailer};

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
    /// Stop the run loop.
    Shutdown,
}

/// A new stream was opened; its reader should be delivered to the application (or routed
/// internally for reserved topics). Carries the sender's identity.
pub struct StreamOpened {
    pub stream_reader: AnyStreamReader,
    pub participant_identity: ParticipantIdentity,
}

/// A "raw chunk received" notification, which is used to trigger
/// the deprecated [RoomEvent:::StreamChunkReceived] event.
pub struct ChunkReceived {
    pub chunk: Chunk,
    pub participant_identity: ParticipantIdentity,
}

/// A "raw trailer received" notification, which is used to trigger
/// the deprecated [RoomEvent:::StreamTrailerReceived] event.
pub struct TrailerReceived {
    pub trailer: Trailer,
    pub participant_identity: ParticipantIdentity,
}

/// An event emitted by [`IncomingStreamManager::run`] for the host crate to surface. The manager
/// stays decoupled from `RoomEvent`; the host maps these onto its own event types.
#[derive(FromVariants)]
pub enum OutputEvent {
    StreamOpened(StreamOpened),
    ChunkReceived(ChunkReceived),
    TrailerReceived(TrailerReceived),
}
