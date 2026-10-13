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

use crate::constants::{
    ATTR_METHOD, ATTR_REQUEST_ID, ATTR_RESPONSE_TIMEOUT_MS, ATTR_VERSION, RPC_REQUEST_TOPIC,
    RPC_VERSION_V1, RPC_VERSION_V2,
};
use crate::transport::{RpcTransport, RpcTransportError};
use crate::types::{PerformRpcData, RpcError, RpcErrorCode, MAX_V1_PAYLOAD_BYTES};
use livekit_common::{ParticipantIdentity, CLIENT_PROTOCOL_DATA_STREAM_RPC};
use livekit_data_stream::api::{StreamReader, StreamTextOptions, TextStreamReader};
use livekit_protocol as proto;
use parking_lot::Mutex;
use semver::Version;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, watch};

/// The oldest server version that supports RPC.
const MIN_RPC_SERVER_VERSION: Version = Version::new(1, 8, 0);

/// How much longer a caller waits for a reply after its recipient is reported gone.
///
/// A participant leaving is announced on the signaling connection, while the ack and response
/// travel on the data channel. Nothing orders those two against each other, so "the recipient
/// left" is not evidence that its reply is not already on the wire — a recipient that leaves
/// *because* of the RPC it just answered (`lk.sip.EndCall`, say) reliably produces that order.
/// Treat the disconnect as a reason to stop waiting soon rather than immediately.
const DISCONNECT_GRACE_PERIOD_DURATION: Duration = Duration::from_secs(2);

/// Generates a random RPC request identifier (UUID v4).
fn create_random_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// An in-flight request, tagged with who it went to so the caller can be told if that
/// participant leaves before replying.
struct Pending<T> {
    destination_identity: ParticipantIdentity,
    tx: oneshot::Sender<T>,
    /// Latches once the destination is known to have left the room. The ack and response
    /// entries of one call share a channel, so a disconnect seen while waiting on the ack is
    /// still waiting for us when the call moves on to the response.
    participant_disconnected: Arc<watch::Sender<bool>>,
}

/// How one leg of a call — the ack, or the response — finished waiting.
enum Wait<T> {
    Received(T),
    Timeout,
    RecipientGone,
}

/// Wait for `rx`, giving up `timeout` from now.
///
/// `participant_disconnected` latching does not end the wait: the recipient has left the room,
/// but its reply may already be in flight, so keep waiting for up to
/// [`DISCONNECT_GRACE_PERIOD_DURATION`] more — never past `timeout` — before reporting the
/// disconnect.
async fn wait_with_disconnect_grace<T>(
    mut rx: oneshot::Receiver<T>,
    participant_disconnected: &mut watch::Receiver<bool>,
    timeout: Duration,
) -> Wait<T> {
    let deadline = tokio::time::Instant::now() + timeout;

    tokio::select! {
        biased;
        // A reply that is already here wins over a disconnect that arrived alongside it.
        received = &mut rx => {
            return match received {
                Ok(value) => Wait::Received(value),
                // Sender dropped without replying: the call was abandoned on our side.
                Err(_) => Wait::RecipientGone,
            };
        }
        // An `Err` here means the call is no longer tracked, which the `rx` arm above covers.
        _ = participant_disconnected.wait_for(|disconnected| *disconnected) => {}
        _ = tokio::time::sleep_until(deadline) => return Wait::Timeout,
    }

    let grace = std::cmp::min(
        DISCONNECT_GRACE_PERIOD_DURATION,
        deadline.saturating_duration_since(tokio::time::Instant::now()),
    );
    match tokio::time::timeout(grace, rx).await {
        Ok(Ok(value)) => Wait::Received(value),
        Ok(Err(_)) | Err(_) => Wait::RecipientGone,
    }
}

/// Manages outgoing RPC calls (caller/client side).
///
/// Tracks pending ACKs and responses, handles v1 packet and v2 data stream
/// transport selection based on the remote participant's client protocol.
pub struct RpcClientManager {
    pending_acks: Mutex<HashMap<String, Pending<()>>>,
    pending_responses: Mutex<HashMap<String, Pending<Result<String, RpcError>>>>,
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
    pub async fn perform_rpc(
        &self,
        data: PerformRpcData,
        transport: &(impl RpcTransport + 'static),
    ) -> Result<String, RpcError> {
        let max_round_trip_latency = data.max_round_trip_latency;
        let min_effective_timeout = Duration::from_millis(1000);

        if let Some(version_str) = transport.server_version() {
            match Version::parse(&version_str) {
                Ok(server_version) if server_version < MIN_RPC_SERVER_VERSION => {
                    return Err(RpcError::built_in(RpcErrorCode::UnsupportedServer, None));
                }
                Ok(_) => {}
                // A version we cannot parse is not evidence that the server is too old, so
                // fail open and let the call proceed rather than rejecting every RPC.
                Err(err) => {
                    log::warn!("Could not parse server version {version_str:?}: {err}");
                }
            }
        }

        // Determine transport version based on remote participant's client_protocol
        let remote_protocol = transport
            .remote_client_protocol(&ParticipantIdentity(data.destination_identity.clone()));
        let use_v2 = remote_protocol >= CLIENT_PROTOCOL_DATA_STREAM_RPC;

        // Only enforce payload size limit for v1 transport
        if !use_v2 && data.payload.len() > MAX_V1_PAYLOAD_BYTES {
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
        let (participant_disconnected_tx, mut participant_disconnected_rx) = watch::channel(false);
        let participant_disconnected_tx = Arc::new(participant_disconnected_tx);
        {
            let mut pending_acks = self.pending_acks.lock();
            let mut pending_responses = self.pending_responses.lock();
            let destination = ParticipantIdentity(data.destination_identity.clone());
            pending_acks.insert(
                id.clone(),
                Pending {
                    destination_identity: destination.clone(),
                    tx: ack_tx,
                    participant_disconnected: participant_disconnected_tx.clone(),
                },
            );
            pending_responses.insert(
                id.clone(),
                Pending {
                    destination_identity: destination,
                    tx: response_tx,
                    participant_disconnected: participant_disconnected_tx,
                },
            );
        }

        let send_result = if use_v2 {
            self.send_v2_request(
                transport,
                data.destination_identity.as_str(),
                &id,
                &data.method,
                &data.payload,
                effective_timeout,
            )
            .await
        } else {
            self.send_v1_request(
                transport,
                data.destination_identity.as_str(),
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
            self.forget(&id);
            log::error!("Failed to publish RPC request: {}", e);
            return Err(e);
        }

        // Wait for the ack
        match wait_with_disconnect_grace(
            ack_rx,
            &mut participant_disconnected_rx,
            max_round_trip_latency,
        )
        .await
        {
            Wait::Timeout => {
                self.forget(&id);
                return Err(RpcError::built_in(RpcErrorCode::ConnectionTimeout, None));
            }
            Wait::RecipientGone => {
                self.forget(&id);
                return Err(RpcError::built_in(RpcErrorCode::RecipientDisconnected, None));
            }
            Wait::Received(()) => {
                // Ack received, continue to wait for response
            }
        }

        // Wait for the response
        match wait_with_disconnect_grace(
            response_rx,
            &mut participant_disconnected_rx,
            data.response_timeout,
        )
        .await
        {
            Wait::Timeout => {
                self.pending_responses.lock().remove(&id);
                Err(RpcError::built_in(RpcErrorCode::ResponseTimeout, None))
            }
            Wait::RecipientGone => {
                self.pending_responses.lock().remove(&id);
                Err(RpcError::built_in(RpcErrorCode::RecipientDisconnected, None))
            }
            // RPC error from remote, forward it
            Wait::Received(Err(e)) => Err(e),
            Wait::Received(Ok(payload)) => Ok(payload),
        }
    }

    /// Stop tracking a call, so a reply that arrives later is ignored rather than delivered.
    fn forget(&self, id: &str) {
        self.pending_acks.lock().remove(id);
        self.pending_responses.lock().remove(id);
    }

    /// Publish a v1 RPC request data packet.
    async fn send_v1_request(
        &self,
        transport: &impl RpcTransport,
        destination_identity: &str,
        id: &str,
        method: &str,
        payload: &str,
        response_timeout: Duration,
    ) -> Result<(), RpcTransportError> {
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

        let options = StreamTextOptions::new_with_topic(RPC_REQUEST_TOPIC)
            .with_attributes(attributes)
            .with_destination_identity(destination_identity);

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
        destination_identity: impl Into<ParticipantIdentity>,
        tx: tokio::sync::oneshot::Sender<Result<String, RpcError>>,
    ) {
        let (participant_disconnected, _rx) = watch::channel(false);
        self.pending_responses.lock().insert(
            request_id,
            Pending {
                destination_identity: destination_identity.into(),
                tx,
                participant_disconnected: Arc::new(participant_disconnected),
            },
        );
    }

    /// Tell every call still waiting on `identity` that it has left the room.
    ///
    /// Each affected caller stops waiting shortly afterwards with `RecipientDisconnected` (see
    /// [`DISCONNECT_GRACE_PERIOD_DURATION`]). Without this, a participant that leaves mid-call
    /// strands its caller until the response timeout expires (15s by default), which then
    /// reports `ResponseTimeout` rather than the disconnect that actually ended the call.
    ///
    /// The pending entries deliberately stay registered through the grace period: a reply sent
    /// just before the recipient left is still in flight on the data channel, and dropping the
    /// entry now would turn that reply into an "unexpected RPC request" and fail a call that
    /// in fact succeeded.
    pub fn handle_participant_disconnected(&self, identity: &ParticipantIdentity) {
        fn notify<T>(pending: &HashMap<String, Pending<T>>, identity: &ParticipantIdentity) {
            for pending in pending.values() {
                if pending.destination_identity == *identity {
                    let _ = pending.participant_disconnected.send(true);
                }
            }
        }

        notify(&self.pending_acks.lock(), identity);
        notify(&self.pending_responses.lock(), identity);
    }

    /// Fail every in-flight call, whoever it was addressed to. Used when the room closes.
    pub fn fail_all_pending(&self) {
        self.pending_acks.lock().clear();

        let stranded: Vec<_> = self.pending_responses.lock().drain().map(|(_, p)| p).collect();
        for pending in stranded {
            let _ =
                pending.tx.send(Err(RpcError::built_in(RpcErrorCode::RecipientDisconnected, None)));
        }
    }

    pub fn handle_incoming_rpc_ack(&self, request_id: String) {
        let mut pending = self.pending_acks.lock();
        if let Some(pending) = pending.remove(&request_id) {
            let _ = pending.tx.send(());
        } else {
            log::error!("Ack received for unexpected RPC request: {}", request_id);
        }
    }

    /// Handle a v1 RPC response packet.
    ///
    /// Also handles error responses for v2 calls, since error responses
    /// always use v1 packets regardless of transport version.
    pub fn handle_v1_response_packet(
        &self,
        request_id: String,
        payload: Option<String>,
        error: Option<proto::RpcError>,
    ) {
        let mut pending = self.pending_responses.lock();
        if let Some(pending) = pending.remove(&request_id) {
            let _ = pending.tx.send(match error {
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
    pub async fn handle_v2_response_stream(&self, reader: TextStreamReader) {
        let request_id =
            reader.info().attributes().get(ATTR_REQUEST_ID).cloned().unwrap_or_default();

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
                if let Some(pending) = pending.remove(&request_id) {
                    let _ = pending.tx.send(Err(RpcError::built_in(
                        RpcErrorCode::ApplicationError,
                        Some(format!("Failed to read response stream: {}", e)),
                    )));
                }
                return;
            }
        };

        let mut pending = self.pending_responses.lock();
        if let Some(pending) = pending.remove(&request_id) {
            let _ = pending.tx.send(Ok(payload));
        } else {
            log::error!("Response stream received for unexpected RPC request: {}", request_id);
        }
    }
}
