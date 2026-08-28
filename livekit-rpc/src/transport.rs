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

//! The seam between the RPC managers and whatever actually moves bytes.

use livekit_common::RemoteParticipantRegistry;
use livekit_data_stream::api::{StreamResult, StreamTextOptions, TextStreamInfo};
use std::future::Future;

/// Error returned by the transport when an RPC data packet fails to send.
///
/// The RPC managers only need a message to attach to the resulting
/// [`RpcError`](crate::api::RpcError); the concrete engine error type stays in the `livekit`
/// crate, which implements [`RpcTransport`] over the RTC engine.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct RpcTransportError(String);

impl RpcTransportError {
    /// Wraps a transport-level failure, preserving its message for the RPC error payload.
    pub fn new(source: impl std::fmt::Display) -> Self {
        Self(source.to_string())
    }
}

/// Transport abstraction for RPC operations.
///
/// Decouples the RPC managers from concrete engine/session types,
/// enabling in-memory unit testing with a mock transport.
pub trait RpcTransport: RemoteParticipantRegistry {
    /// Send a data packet (used for v1 RPC packets and ACKs).
    fn publish_data(
        &self,
        data: livekit_protocol::DataPacket,
    ) -> impl Future<Output = Result<(), RpcTransportError>> + Send;

    /// Send text as a data stream (used for v2 RPC requests and responses).
    fn send_text(
        &self,
        text: &str,
        options: StreamTextOptions,
    ) -> impl Future<Output = StreamResult<TextStreamInfo>> + Send;

    /// Get the server version string, if available.
    fn server_version(&self) -> Option<String>;
}
