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

use super::client::{publish_rpc_ack, publish_rpc_response};
use super::{
    RpcError, RpcErrorCode, RpcInvocationData, RpcTransport, ATTR_METHOD, ATTR_REQUEST_ID,
    ATTR_RESPONSE_TIMEOUT_MS, ATTR_VERSION, MAX_PAYLOAD_BYTES, RPC_RESPONSE_TOPIC, RPC_VERSION_V1,
    RPC_VERSION_V2,
};
use crate::data_stream::{StreamReader, StreamTextOptions, TextStreamReader};
use crate::room::id::ParticipantIdentity;
use parking_lot::Mutex;
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc, time::Duration};

pub(crate) type RpcHandlerFn = Arc<
    dyn Fn(RpcInvocationData) -> Pin<Box<dyn Future<Output = Result<String, RpcError>> + Send>>
        + Send
        + Sync,
>;

/// Parameters for [`RpcServerManager::handle_request`].
pub struct HandleRequestOptions {
    pub caller_identity: ParticipantIdentity,
    pub request_id: String,
    pub method: String,
    pub payload: String,
    pub response_timeout: Duration,
    pub version: u32,
}

/// Manages incoming RPC requests (handler/server side).
///
/// Stores registered method handlers and dispatches incoming requests
/// to the appropriate handler. Handles both v1 packet and v2 data stream
/// request formats.
pub struct RpcServerManager {
    handlers: Mutex<HashMap<String, RpcHandlerFn>>,
}

impl RpcServerManager {
    pub fn new() -> Self {
        Self { handlers: Mutex::new(HashMap::new()) }
    }

    pub fn register_method(
        &self,
        method: String,
        handler: impl Fn(RpcInvocationData) -> Pin<Box<dyn Future<Output = Result<String, RpcError>> + Send>>
            + Send
            + Sync
            + 'static,
    ) {
        self.handlers.lock().insert(method, Arc::new(handler));
    }

    pub fn unregister_method(&self, method: &str) {
        self.handlers.lock().remove(method);
    }

    pub(crate) fn get_handler(&self, method: &str) -> Option<RpcHandlerFn> {
        self.handlers.lock().get(method).cloned()
    }

    /// Handle an incoming v1 RPC request (received as a DataPacket).
    ///
    /// Sends ACK, invokes the registered handler, and sends the response
    /// as a v1 RPC response packet.
    pub(crate) async fn handle_request(
        &self,
        options: HandleRequestOptions,
        transport: &(impl RpcTransport + 'static),
    ) {
        let HandleRequestOptions {
            caller_identity,
            request_id,
            method,
            payload,
            response_timeout,
            version,
        } = options;

        // Send ACK immediately
        if let Err(e) = publish_rpc_ack(transport, &caller_identity.0, &request_id).await {
            log::error!("Failed to publish RPC ACK: {:?}", e);
        }

        let response = if version != RPC_VERSION_V1 {
            Err(RpcError::built_in(RpcErrorCode::UnsupportedVersion, None))
        } else {
            self.invoke_handler(&caller_identity, &request_id, &method, &payload, response_timeout)
                .await
        };

        let (resp_payload, error) = match response {
            Ok(response_payload) if response_payload.len() <= MAX_PAYLOAD_BYTES => {
                (Some(response_payload), None)
            }
            Ok(_) => (
                None,
                Some(RpcError::built_in(RpcErrorCode::ResponsePayloadTooLarge, None).to_proto()),
            ),
            Err(e) => (None, Some(e.to_proto())),
        };

        if let Err(e) =
            publish_rpc_response(transport, &caller_identity.0, &request_id, resp_payload, error)
                .await
        {
            log::error!("Failed to publish RPC response: {:?}", e);
        }
    }

    /// Handle an incoming v2 RPC request (received as a data stream).
    ///
    /// Parses request metadata from stream attributes, sends ACK,
    /// invokes the handler, and sends the response. Success responses
    /// use a v2 data stream; error responses always use v1 packets.
    pub(crate) async fn handle_request_stream(
        &self,
        reader: TextStreamReader,
        caller_identity: ParticipantIdentity,
        transport: &(impl RpcTransport + 'static),
    ) {
        let attrs = &reader.info().attributes;

        let request_id = attrs.get(ATTR_REQUEST_ID).cloned().unwrap_or_default();
        let method = attrs.get(ATTR_METHOD).cloned().unwrap_or_default();
        let response_timeout_ms: u64 =
            attrs.get(ATTR_RESPONSE_TIMEOUT_MS).and_then(|v| v.parse().ok()).unwrap_or(15000);
        let version: u32 = attrs.get(ATTR_VERSION).and_then(|v| v.parse().ok()).unwrap_or(0);

        let response_timeout = Duration::from_millis(response_timeout_ms);

        // Send ACK immediately (always v1 packet)
        if let Err(e) = publish_rpc_ack(transport, &caller_identity.0, &request_id).await {
            log::error!("Failed to publish RPC ACK: {:?}", e);
        }

        if version != RPC_VERSION_V2 {
            let error = RpcError::built_in(RpcErrorCode::UnsupportedVersion, None);
            let _ = publish_rpc_response(
                transport,
                &caller_identity.0,
                &request_id,
                None,
                Some(error.to_proto()),
            )
            .await;
            return;
        }

        // Read the full payload from the stream
        let payload = match reader.read_all().await {
            Ok(payload) => payload,
            Err(e) => {
                log::error!("Failed to read RPC v2 request stream: {:?}", e);
                let error = RpcError::built_in(
                    RpcErrorCode::ApplicationError,
                    Some(format!("Failed to read request stream: {}", e)),
                );
                let _ = publish_rpc_response(
                    transport,
                    &caller_identity.0,
                    &request_id,
                    None,
                    Some(error.to_proto()),
                )
                .await;
                return;
            }
        };

        let response = self
            .invoke_handler(&caller_identity, &request_id, &method, &payload, response_timeout)
            .await;

        match response {
            Ok(response_payload) => {
                // Success: send response as v2 data stream
                let mut attributes = HashMap::new();
                attributes.insert(ATTR_REQUEST_ID.to_string(), request_id.clone());

                let options = StreamTextOptions {
                    topic: RPC_RESPONSE_TOPIC.to_string(),
                    attributes,
                    destination_identities: vec![caller_identity.clone()],
                    ..Default::default()
                };

                if let Err(e) = transport.send_text(&response_payload, options).await {
                    log::error!("Failed to send RPC v2 response stream: {:?}", e);
                    // Fall back to error via v1 packet
                    let error = RpcError::built_in(RpcErrorCode::SendFailed, Some(e.to_string()));
                    let _ = publish_rpc_response(
                        transport,
                        &caller_identity.0,
                        &request_id,
                        None,
                        Some(error.to_proto()),
                    )
                    .await;
                }
            }
            Err(e) => {
                // Error: always send as v1 packet
                if let Err(send_err) = publish_rpc_response(
                    transport,
                    &caller_identity.0,
                    &request_id,
                    None,
                    Some(e.to_proto()),
                )
                .await
                {
                    log::error!("Failed to publish RPC error response: {:?}", send_err);
                }
            }
        }
    }

    /// Invoke a registered handler for an RPC method, with error handling.
    async fn invoke_handler(
        &self,
        caller_identity: &ParticipantIdentity,
        request_id: &str,
        method: &str,
        payload: &str,
        response_timeout: Duration,
    ) -> Result<String, RpcError> {
        let handler = self.get_handler(method);

        match handler {
            Some(handler) => {
                let caller_id = caller_identity.clone();
                let req_id = request_id.to_string();
                let req_payload = payload.to_string();
                match tokio::task::spawn(async move {
                    handler(RpcInvocationData {
                        request_id: req_id,
                        caller_identity: caller_id,
                        payload: req_payload,
                        response_timeout,
                    })
                    .await
                })
                .await
                {
                    Ok(result) => result,
                    Err(e) => {
                        log::error!("RPC method handler returned an error: {:?}", e);
                        Err(RpcError::built_in(RpcErrorCode::ApplicationError, None))
                    }
                }
            }
            None => Err(RpcError::built_in(RpcErrorCode::UnsupportedMethod, None)),
        }
    }
}
