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

//! UniFFI bindings for data streams v2 from [`livekit-data-stream`].
//!
//! Mirrors the [`crate::data_track`] pattern:
//! - [`incoming::IncomingDataStreamManager`] wraps the incoming actor. Packets are fed in via a
//!   synchronous `handle_packet_received` (safe to call from a native data-channel callback), and
//!   opened readers / v1 back-compat events are pushed out through a foreign delegate.
//! - [`outgoing::OutgoingDataStreamManager`] wraps the outgoing manager as an object with async
//!   `send_*`/`stream_*` methods. Outbound packets are handed to a foreign delegate, and remote
//!   participant protocol/capabilities are read through a foreign registry callback.

pub mod common;
pub mod incoming;
pub mod outgoing;

#[cfg(test)]
mod tests;
