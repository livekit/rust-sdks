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

use super::{
    PerformRpcData, RpcError, RpcErrorCode, RpcTransport, ATTR_METHOD, ATTR_REQUEST_ID,
    ATTR_RESPONSE_TIMEOUT_MS, ATTR_VERSION, MAX_PAYLOAD_BYTES, RPC_REQUEST_TOPIC, RPC_VERSION_V1,
    RPC_VERSION_V2,
};
use crate::data_stream::{StreamReader, StreamTextOptions, TextStreamReader};
use crate::room::id::ParticipantIdentity;
use libwebrtc::native::create_random_uuid;
use livekit_api::signal_client::CLIENT_PROTOCOL_DATA_STREAM_RPC;
use livekit_protocol as proto;
use parking_lot::Mutex;
use semver::Version;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::oneshot;

/// Manages outgoing RPC calls (caller/client side).
///
/// Tracks pending ACKs and responses, handles v1 packet and v2 data stream
/// transport selection based on the remote participant's client protocol.
pub struct RpcClientManager {
    pending_acks: Mutex<HashMap<String, oneshot::Sender<()>>>,
    pending_responses: Mutex<HashMap<String, oneshot::Sender<Result<String, RpcError>>>>,
}

impl RpcClientManager {
    pub fn new() -> Self {
        Self {
            pending_acks: Mutex::new(HashMap::new()),
            pending_responses: Mutex::new(HashMap::new()),
        }
    }

    /// Perform an RPC call to a remote participant.
    ///
    /// Selects v1 (data packet) or v2 (data stream) transport based on
    /// the remote participant's client_protocol.
    pub(crate) async fn perform_rpc(
        &self,
        data: PerformRpcData,
        transport: &(impl RpcTransport + 'static),
    ) -> Result<String, RpcError> {
        let max_round_trip_latency = Duration::from_millis(7000);
        let min_effective_timeout = Duration::from_millis(1000);

        if let Some(version_str) = transport.server_version() {
            let server_version = Version::parse(&version_str).unwrap();
            let min_required_version = Version::parse("1.8.0").unwrap();
            if server_version < min_required_version {
                return Err(RpcError::built_in(RpcErrorCode::UnsupportedServer, None));
            }
        }

        // Determine transport version based on remote participant's client_protocol
        let remote_protocol = transport
            .remote_client_protocol(&ParticipantIdentity(data.destination_identity.clone()));
        let use_v2 = remote_protocol >= CLIENT_PROTOCOL_DATA_STREAM_RPC;

        // Only enforce payload size limit for v1 transport
        if !use_v2 && data.payload.len() > MAX_PAYLOAD_BYTES {
            return Err(RpcError::built_in(RpcErrorCode::RequestPayloadTooLarge, None));
        }

        let id = create_random_uuid();
        let (ack_tx, ack_rx) = oneshot::channel();
        let (response_tx, response_rx) = oneshot::channel();
        let effective_timeout = std::cmp::max(
            data.response_timeout.saturating_sub(max_round_trip_latency),
            min_effective_timeout,
        );

        // Register channels BEFORE sending the request to avoid race condition
        // where the response arrives before we've registered the handlers
        {
            let mut pending_acks = self.pending_acks.lock();
            let mut pending_responses = self.pending_responses.lock();
            pending_acks.insert(id.clone(), ack_tx);
            pending_responses.insert(id.clone(), response_tx);
        }

        let send_result = if use_v2 {
            self.send_v2_request(
                transport,
                &data.destination_identity,
                &id,
                &data.method,
                &data.payload,
                effective_timeout,
            )
            .await
        } else {
            self.send_v1_request(
                transport,
                &data.destination_identity,
                &id,
                &data.method,
                &data.payload,
                effective_timeout,
            )
            .await
            .map_err(|e| RpcError::built_in(RpcErrorCode::SendFailed, Some(e.to_string())))
        };

        if let Err(e) = send_result {
            // Clean up on failure
            let mut pending_acks = self.pending_acks.lock();
            let mut pending_responses = self.pending_responses.lock();
            pending_acks.remove(&id);
            pending_responses.remove(&id);
            log::error!("Failed to publish RPC request: {}", e);
            return Err(e);
        }

        // Wait for ack timeout
        match tokio::time::timeout(max_round_trip_latency, ack_rx).await {
            Err(_) => {
                let mut pending_acks = self.pending_acks.lock();
                let mut pending_responses = self.pending_responses.lock();
                pending_acks.remove(&id);
                pending_responses.remove(&id);
                return Err(RpcError::built_in(RpcErrorCode::ConnectionTimeout, None));
            }
            Ok(_) => {
                // Ack received, continue to wait for response
            }
        }

        // Wait for response timeout
        let response = match tokio::time::timeout(data.response_timeout, response_rx).await {
            Err(_) => {
                self.pending_responses.lock().remove(&id);
                return Err(RpcError::built_in(RpcErrorCode::ResponseTimeout, None));
            }
            Ok(result) => result,
        };

        match response {
            Err(_) => {
                // Channel closed — sender dropped (e.g. disconnect)
                Err(RpcError::built_in(RpcErrorCode::RecipientDisconnected, None))
            }
            Ok(Err(e)) => {
                // RPC error from remote, forward it
                Err(e)
            }
            Ok(Ok(payload)) => {
                // Successful response
                Ok(payload)
            }
        }
    }

    /// Publish a v1 RPC request data packet.
    pub(crate) async fn send_v1_request(
        &self,
        transport: &impl RpcTransport,
        destination_identity: &str,
        id: &str,
        method: &str,
        payload: &str,
        response_timeout: Duration,
    ) -> Result<(), crate::room::RoomError> {
        let rpc_request_message = proto::RpcRequest {
            id: id.to_string(),
            method: method.to_string(),
            payload: payload.to_string(),
            response_timeout_ms: response_timeout.as_millis() as u32,
            version: RPC_VERSION_V1,
            ..Default::default()
        };

        let data = proto::DataPacket {
            value: Some(proto::data_packet::Value::RpcRequest(rpc_request_message)),
            destination_identities: vec![destination_identity.to_string()],
            ..Default::default()
        };

        transport.publish_data(data).await
    }

    /// Send an RPC request as a v2 text data stream.
    async fn send_v2_request(
        &self,
        transport: &impl RpcTransport,
        destination_identity: &str,
        id: &str,
        method: &str,
        payload: &str,
        response_timeout: Duration,
    ) -> Result<(), RpcError> {
        let mut attributes = HashMap::new();
        attributes.insert(ATTR_REQUEST_ID.to_string(), id.to_string());
        attributes.insert(ATTR_METHOD.to_string(), method.to_string());
        attributes
            .insert(ATTR_RESPONSE_TIMEOUT_MS.to_string(), response_timeout.as_millis().to_string());
        attributes.insert(ATTR_VERSION.to_string(), RPC_VERSION_V2.to_string());

        let options = StreamTextOptions {
            topic: RPC_REQUEST_TOPIC.to_string(),
            attributes,
            destination_identities: vec![ParticipantIdentity(destination_identity.to_string())],
            ..Default::default()
        };

        transport
            .send_text(payload, options)
            .await
            .map(|_| ())
            .map_err(|e| RpcError::built_in(RpcErrorCode::SendFailed, Some(e.to_string())))
    }

    /// Drop the pending response sender for a request, simulating a disconnect.
    #[cfg(test)]
    pub(crate) fn drop_pending_response(&self, request_id: &str) {
        self.pending_responses.lock().remove(request_id);
    }

    /// Register a pending response channel for testing.
    #[cfg(test)]
    pub(crate) fn insert_pending_response(
        &self,
        request_id: String,
        tx: tokio::sync::oneshot::Sender<Result<String, RpcError>>,
    ) {
        self.pending_responses.lock().insert(request_id, tx);
    }

    pub(crate) fn handle_ack(&self, request_id: String) {
        let mut pending = self.pending_acks.lock();
        if let Some(tx) = pending.remove(&request_id) {
            let _ = tx.send(());
        } else {
            log::error!("Ack received for unexpected RPC request: {}", request_id);
        }
    }

    /// Handle a v1 RPC response packet.
    ///
    /// Also handles error responses for v2 calls, since error responses
    /// always use v1 packets regardless of transport version.
    pub(crate) fn handle_response(
        &self,
        request_id: String,
        payload: Option<String>,
        error: Option<proto::RpcError>,
    ) {
        let mut pending = self.pending_responses.lock();
        if let Some(tx) = pending.remove(&request_id) {
            let _ = tx.send(match error {
                Some(e) => Err(RpcError::from_proto(e)),
                None => Ok(payload.unwrap_or_default()),
            });
        } else {
            log::error!("Response received for unexpected RPC request: {}", request_id);
        }
    }

    /// Handle a v2 RPC success response received as a data stream.
    ///
    /// Success responses between v2 clients arrive as text data streams
    /// on the `lk.rpc_response` topic. Error responses always arrive
    /// as v1 packets and are handled by `handle_response`.
    pub(crate) async fn handle_response_stream(&self, reader: TextStreamReader) {
        let request_id = reader.info().attributes.get(ATTR_REQUEST_ID).cloned().unwrap_or_default();

        if request_id.is_empty() {
            log::error!("RPC v2 response stream missing request_id attribute");
            return;
        }

        let payload = match reader.read_all().await {
            Ok(payload) => payload,
            Err(e) => {
                log::error!("Failed to read RPC v2 response stream: {:?}", e);
                // Resolve with error so the caller doesn't hang
                let mut pending = self.pending_responses.lock();
                if let Some(tx) = pending.remove(&request_id) {
                    let _ = tx.send(Err(RpcError::built_in(
                        RpcErrorCode::ApplicationError,
                        Some(format!("Failed to read response stream: {}", e)),
                    )));
                }
                return;
            }
        };

        let mut pending = self.pending_responses.lock();
        if let Some(tx) = pending.remove(&request_id) {
            let _ = tx.send(Ok(payload));
        } else {
            log::error!("Response stream received for unexpected RPC request: {}", request_id);
        }
    }
}

/// Publish a v1 RPC response data packet.
pub(crate) async fn publish_rpc_response(
    transport: &impl RpcTransport,
    destination_identity: &str,
    request_id: &str,
    payload: Option<String>,
    error: Option<proto::RpcError>,
) -> Result<(), crate::room::RoomError> {
    let rpc_response_message = proto::RpcResponse {
        request_id: request_id.to_string(),
        value: Some(match error {
            Some(error) => proto::rpc_response::Value::Error(error),
            None => proto::rpc_response::Value::Payload(payload.unwrap()),
        }),
        ..Default::default()
    };

    let data = proto::DataPacket {
        value: Some(proto::data_packet::Value::RpcResponse(rpc_response_message)),
        destination_identities: vec![destination_identity.to_string()],
        ..Default::default()
    };

    transport.publish_data(data).await
}

/// Publish a v1 RPC ack data packet.
pub(crate) async fn publish_rpc_ack(
    transport: &impl RpcTransport,
    destination_identity: &str,
    request_id: &str,
) -> Result<(), crate::room::RoomError> {
    let rpc_ack_message =
        proto::RpcAck { request_id: request_id.to_string(), ..Default::default() };

    let data = proto::DataPacket {
        value: Some(proto::data_packet::Value::RpcAck(rpc_ack_message)),
        destination_identities: vec![destination_identity.to_string()],
        ..Default::default()
    };

    transport.publish_data(data).await
}
