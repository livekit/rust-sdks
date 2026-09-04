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

//! The production [`RpcTransport`] implementation.
//!
//! The RPC implementation itself lives in the `livekit-rpc` crate; this is the bridge that
//! backs it with a live [`RoomSession`](super::RoomSession) — the RTC engine for v1 packets,
//! the outgoing data-stream manager for v2 streams, and the signal client for the server
//! version.

use livekit_common::{ClientCapability, ParticipantIdentity, RemoteParticipantRegistry};
use livekit_data_stream::api::{StreamResult, StreamTextOptions, TextStreamInfo};
use livekit_rpc::backend::{RpcTransport, RpcTransportError};
use std::sync::Arc;

/// Production implementation of `RpcTransport` backed by a `RoomSession`.
pub(crate) struct SessionTransport(pub(crate) Arc<super::RoomSession>);

impl RemoteParticipantRegistry for SessionTransport {
    fn remote_client_protocol(&self, identity: &ParticipantIdentity) -> i32 {
        self.0.remote_client_protocol(identity)
    }

    fn remote_capabilities(&self, identity: &ParticipantIdentity) -> Vec<ClientCapability> {
        self.0.remote_capabilities(identity)
    }

    fn remote_identities(&self) -> Vec<ParticipantIdentity> {
        self.0.remote_identities()
    }
}

impl RpcTransport for SessionTransport {
    async fn publish_data(
        &self,
        data: livekit_protocol::DataPacket,
    ) -> Result<(), RpcTransportError> {
        self.0
            .rtc_engine
            .publish_data(data, crate::DataPacketKind::Reliable, false)
            .await
            .map_err(RpcTransportError::new)
    }

    async fn send_text(
        &self,
        text: &str,
        options: StreamTextOptions,
    ) -> StreamResult<TextStreamInfo> {
        self.0.outgoing_stream_manager.send_text(text, options, self.0.as_ref()).await
    }

    fn server_version(&self) -> Option<String> {
        self.0
            .rtc_engine
            .session()
            .signal_client()
            .join_response()
            .server_info
            .and_then(|info| (!info.version.is_empty()).then(|| info.version))
    }
}
