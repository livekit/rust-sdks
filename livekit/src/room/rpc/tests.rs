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

use super::*;
use crate::data_stream::{
    OperationType, StreamResult, StreamTextOptions, TextStreamInfo, TextStreamReader,
};
use crate::e2ee::EncryptionType;
use crate::room::id::ParticipantIdentity;
use crate::room::RoomError;
use bytes::Bytes;
use chrono::Utc;
use livekit_api::signal_client::{CLIENT_PROTOCOL_DATA_STREAM_RPC, CLIENT_PROTOCOL_DEFAULT};
use livekit_protocol as proto;
use parking_lot::Mutex as ParkingMutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Notify};

// ---------------------------------------------------------------------------
// Mock transport
// ---------------------------------------------------------------------------

/// Captures all outgoing packets and text streams for assertion.
struct MockTransport {
    sent_packets: Arc<ParkingMutex<Vec<proto::DataPacket>>>,
    sent_texts: Arc<ParkingMutex<Vec<(String, StreamTextOptions)>>>,
    packet_sent: Arc<Notify>,
    text_sent: Arc<Notify>,
    remote_protocols: HashMap<String, i32>,
    server_ver: Option<String>,
}

impl MockTransport {
    fn new() -> Self {
        Self {
            sent_packets: Default::default(),
            sent_texts: Default::default(),
            packet_sent: Arc::new(Notify::new()),
            text_sent: Arc::new(Notify::new()),
            remote_protocols: HashMap::new(),
            server_ver: Some("1.8.0".to_string()),
        }
    }

    fn with_remote_protocol(mut self, identity: &str, protocol: i32) -> Self {
        self.remote_protocols.insert(identity.to_string(), protocol);
        self
    }

    /// Wait until at least one packet has been sent.
    async fn wait_for_packet(&self) {
        self.packet_sent.notified().await;
    }

    /// Wait until at least one text stream has been sent.
    async fn wait_for_text(&self) {
        self.text_sent.notified().await;
    }

    /// Return all sent packets.
    fn packets(&self) -> Vec<proto::DataPacket> {
        self.sent_packets.lock().clone()
    }

    /// Return all sent text streams as (body, options).
    fn texts(&self) -> Vec<(String, StreamTextOptions)> {
        self.sent_texts.lock().clone()
    }

    /// Count packets matching a predicate on their `value`.
    fn count_packets<F: Fn(&proto::data_packet::Value) -> bool>(&self, f: F) -> usize {
        self.packets().iter().filter(|p| p.value.as_ref().map_or(false, &f)).count()
    }

    /// Extract the request ID from the first RPC request packet or text stream.
    fn extract_request_id(&self) -> String {
        // Try v1 packets first
        for p in self.packets() {
            if let Some(proto::data_packet::Value::RpcRequest(req)) = &p.value {
                return req.id.clone();
            }
        }
        // Try v2 text streams
        for (_, opts) in self.texts() {
            if opts.topic == RPC_REQUEST_TOPIC {
                if let Some(id) = opts.attributes.get(ATTR_REQUEST_ID) {
                    return id.clone();
                }
            }
        }
        panic!("No RPC request found in mock transport");
    }
}

impl RpcTransport for MockTransport {
    async fn publish_data(&self, data: proto::DataPacket) -> Result<(), RoomError> {
        self.sent_packets.lock().push(data);
        self.packet_sent.notify_waiters();
        Ok(())
    }

    async fn send_text(
        &self,
        text: &str,
        options: StreamTextOptions,
    ) -> StreamResult<TextStreamInfo> {
        self.sent_texts.lock().push((text.to_string(), options.clone()));
        self.text_sent.notify_waiters();
        Ok(TextStreamInfo {
            id: "mock-stream-id".to_string(),
            topic: options.topic,
            timestamp: Utc::now(),
            total_length: Some(text.len() as u64),
            attributes: options.attributes,
            mime_type: "text/plain".to_string(),
            operation_type: OperationType::Create,
            version: 0,
            reply_to_stream_id: None,
            attached_stream_ids: vec![],
            generated: false,
            encryption_type: EncryptionType::None,
        })
    }

    fn remote_client_protocol(&self, identity: &ParticipantIdentity) -> i32 {
        self.remote_protocols.get(&identity.0).copied().unwrap_or(CLIENT_PROTOCOL_DEFAULT)
    }

    fn server_version(&self) -> Option<String> {
        self.server_ver.clone()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_text_reader(
    text: &str,
    attributes: HashMap<String, String>,
    topic: &str,
) -> TextStreamReader {
    let (tx, rx) = mpsc::unbounded_channel();
    tx.send(Ok(Bytes::from(text.to_string()))).unwrap();
    drop(tx); // close the stream
    TextStreamReader::new_for_test(
        TextStreamInfo {
            id: "test-stream".to_string(),
            topic: topic.to_string(),
            timestamp: Utc::now(),
            total_length: Some(text.len() as u64),
            attributes,
            mime_type: "text/plain".to_string(),
            operation_type: OperationType::Create,
            version: 0,
            reply_to_stream_id: None,
            attached_stream_ids: vec![],
            generated: false,
            encryption_type: EncryptionType::None,
        },
        rx,
    )
}

fn v2_request_attrs(request_id: &str, method: &str, timeout_ms: u64) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    attrs.insert(ATTR_REQUEST_ID.to_string(), request_id.to_string());
    attrs.insert(ATTR_METHOD.to_string(), method.to_string());
    attrs.insert(ATTR_RESPONSE_TIMEOUT_MS.to_string(), timeout_ms.to_string());
    attrs.insert(ATTR_VERSION.to_string(), "2".to_string());
    attrs
}

fn v2_response_attrs(request_id: &str) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    attrs.insert(ATTR_REQUEST_ID.to_string(), request_id.to_string());
    attrs
}

fn is_rpc_request_packet(v: &proto::data_packet::Value) -> bool {
    matches!(v, proto::data_packet::Value::RpcRequest(_))
}

fn is_rpc_response_packet(v: &proto::data_packet::Value) -> bool {
    matches!(v, proto::data_packet::Value::RpcResponse(_))
}

fn is_rpc_ack_packet(v: &proto::data_packet::Value) -> bool {
    matches!(v, proto::data_packet::Value::RpcAck(_))
}

fn extract_response_error(transport: &MockTransport) -> Option<proto::RpcError> {
    for p in transport.packets() {
        if let Some(proto::data_packet::Value::RpcResponse(resp)) = &p.value {
            if let Some(proto::rpc_response::Value::Error(e)) = &resp.value {
                return Some(e.clone());
            }
        }
    }
    None
}

/// Run `perform_rpc` in a background task and return a handle.
///
/// Uses `Arc` to share the client and transport safely across the spawn boundary.
async fn spawn_perform_rpc(
    client: Arc<RpcClientManager>,
    transport: Arc<MockTransport>,
    data: PerformRpcData,
) -> tokio::task::JoinHandle<Result<String, RpcError>> {
    tokio::spawn(async move { client.perform_rpc(data, &*transport).await })
}

// =========================================================================
// v2 -> v2 tests (both sides support data streams)
// =========================================================================

/// Spec #1: Caller happy path (short payload) — v2 data stream used.
#[tokio::test]
async fn test_v2_v2_caller_happy_path_short() {
    let client = Arc::new(RpcClientManager::new());
    let transport = Arc::new(
        MockTransport::new().with_remote_protocol("dest", CLIENT_PROTOCOL_DATA_STREAM_RPC),
    );

    let handle = spawn_perform_rpc(
        client.clone(),
        transport.clone(),
        PerformRpcData {
            destination_identity: "dest".into(),
            method: "greet".into(),
            payload: "hello".into(),
            response_timeout: Duration::from_secs(5),
        },
    )
    .await;

    // Wait for the request to be sent
    transport.wait_for_text().await;

    // Verify: sent as v2 data stream, NOT a v1 packet
    assert_eq!(transport.count_packets(is_rpc_request_packet), 0);
    assert_eq!(transport.texts().len(), 1);
    let (body, opts) = &transport.texts()[0];
    assert_eq!(opts.topic, RPC_REQUEST_TOPIC);
    assert_eq!(body, "hello");
    assert_eq!(opts.attributes.get(ATTR_VERSION).unwrap(), "2");

    let request_id = transport.extract_request_id();

    // Simulate ACK + response
    client.handle_ack(request_id.clone());
    client.handle_response(request_id, Some("world".into()), None);

    let result = handle.await.unwrap();
    assert_eq!(result.unwrap(), "world");
}

/// Spec #2: Caller happy path (large payload > 15 KB) — no size error.
#[tokio::test]
async fn test_v2_v2_caller_happy_path_large_payload() {
    let client = Arc::new(RpcClientManager::new());
    let transport = Arc::new(
        MockTransport::new().with_remote_protocol("dest", CLIENT_PROTOCOL_DATA_STREAM_RPC),
    );

    let large_payload = "x".repeat(20_000);
    let handle = spawn_perform_rpc(
        client.clone(),
        transport.clone(),
        PerformRpcData {
            destination_identity: "dest".into(),
            method: "big".into(),
            payload: large_payload,
            response_timeout: Duration::from_secs(5),
        },
    )
    .await;

    transport.wait_for_text().await;

    let (body, _) = &transport.texts()[0];
    assert_eq!(body.len(), 20_000);

    let request_id = transport.extract_request_id();
    client.handle_ack(request_id.clone());
    client.handle_response(request_id, Some("ok".into()), None);

    let result = handle.await.unwrap();
    assert_eq!(result.unwrap(), "ok");
}

/// Spec #3: Handler happy path — response sent via v2 data stream.
#[tokio::test]
async fn test_v2_v2_handler_happy_path() {
    let server = RpcServerManager::new();
    let transport = MockTransport::new();

    server.register_method("echo".to_string(), |data| Box::pin(async move { Ok(data.payload) }));

    let reader = make_text_reader(
        "request-body",
        v2_request_attrs("req-1", "echo", 5000),
        RPC_REQUEST_TOPIC,
    );

    server.handle_request_stream(reader, ParticipantIdentity("caller".into()), &transport).await;

    // ACK should be sent as v1 packet
    assert_eq!(transport.count_packets(is_rpc_ack_packet), 1);

    // Success response should be sent as v2 data stream, NOT a v1 packet
    assert_eq!(transport.count_packets(is_rpc_response_packet), 0);
    assert_eq!(transport.texts().len(), 1);
    let (body, opts) = &transport.texts()[0];
    assert_eq!(opts.topic, RPC_RESPONSE_TOPIC);
    assert_eq!(body, "request-body"); // echo
    assert_eq!(opts.attributes.get(ATTR_REQUEST_ID).unwrap(), "req-1");
}

/// Spec #4: Unhandled error in handler — error sent via v1 packet.
#[tokio::test]
async fn test_v2_v2_handler_unhandled_error() {
    let server = RpcServerManager::new();
    let transport = MockTransport::new();

    server.register_method("crash".to_string(), |_data| {
        Box::pin(async move {
            panic!("handler panic");
        })
    });

    let reader =
        make_text_reader("payload", v2_request_attrs("req-2", "crash", 5000), RPC_REQUEST_TOPIC);

    server.handle_request_stream(reader, ParticipantIdentity("caller".into()), &transport).await;

    // Error responses always use v1 packets, even between v2 clients
    assert_eq!(transport.count_packets(is_rpc_response_packet), 1);
    assert_eq!(transport.texts().len(), 0); // no data stream response

    let err = extract_response_error(&transport).unwrap();
    assert_eq!(err.code, RpcErrorCode::ApplicationError as u32);
}

/// Spec #5: RpcError passthrough in handler — custom error code preserved.
#[tokio::test]
async fn test_v2_v2_handler_rpc_error_passthrough() {
    let server = RpcServerManager::new();
    let transport = MockTransport::new();

    server.register_method("fail".to_string(), |_data| {
        Box::pin(async move { Err(RpcError::new(101, "custom".into(), Some("data".into()))) })
    });

    let reader =
        make_text_reader("payload", v2_request_attrs("req-3", "fail", 5000), RPC_REQUEST_TOPIC);

    server.handle_request_stream(reader, ParticipantIdentity("caller".into()), &transport).await;

    // Error sent as v1 packet
    let err = extract_response_error(&transport).unwrap();
    assert_eq!(err.code, 101);
    assert_eq!(err.message, "custom");
}

/// Spec #6: Response timeout — caller gives up after timeout.
#[tokio::test]
async fn test_v2_v2_response_timeout() {
    let client = RpcClientManager::new();
    let transport =
        MockTransport::new().with_remote_protocol("dest", CLIENT_PROTOCOL_DATA_STREAM_RPC);

    // Very short timeout — no ack or response will arrive.
    // The ack timeout (7s) is larger than response_timeout (50ms),
    // so connection timeout fires.
    let result = client
        .perform_rpc(
            PerformRpcData {
                destination_identity: "dest".into(),
                method: "slow".into(),
                payload: "x".into(),
                response_timeout: Duration::from_millis(50),
            },
            &transport,
        )
        .await;

    let err = result.unwrap_err();
    assert_eq!(err.code, RpcErrorCode::ConnectionTimeout as u32);
}

/// Spec #7: Error response — v1 error packet received by v2 caller.
#[tokio::test]
async fn test_v2_v2_error_response() {
    let client = Arc::new(RpcClientManager::new());
    let transport = Arc::new(
        MockTransport::new().with_remote_protocol("dest", CLIENT_PROTOCOL_DATA_STREAM_RPC),
    );

    let handle = spawn_perform_rpc(
        client.clone(),
        transport.clone(),
        PerformRpcData {
            destination_identity: "dest".into(),
            method: "err".into(),
            payload: "x".into(),
            response_timeout: Duration::from_secs(5),
        },
    )
    .await;

    transport.wait_for_text().await;
    let request_id = transport.extract_request_id();

    client.handle_ack(request_id.clone());
    // Error response arrives as v1 packet (per spec)
    client.handle_response(
        request_id,
        None,
        Some(proto::RpcError { code: 101, message: "nope".into(), data: "details".into() }),
    );

    let result = handle.await.unwrap();
    let err = result.unwrap_err();
    assert_eq!(err.code, 101);
    assert_eq!(err.message, "nope");
}

/// Spec #8: Participant disconnection — channel dropped before response.
#[tokio::test]
async fn test_v2_v2_participant_disconnection() {
    let client = Arc::new(RpcClientManager::new());
    let transport = Arc::new(
        MockTransport::new().with_remote_protocol("dest", CLIENT_PROTOCOL_DATA_STREAM_RPC),
    );

    let handle = spawn_perform_rpc(
        client.clone(),
        transport.clone(),
        PerformRpcData {
            destination_identity: "dest".into(),
            method: "dc".into(),
            payload: "x".into(),
            response_timeout: Duration::from_secs(5),
        },
    )
    .await;

    transport.wait_for_text().await;
    let request_id = transport.extract_request_id();

    // ACK arrives, then the responder disconnects (response channel dropped)
    client.handle_ack(request_id.clone());
    // Simulate disconnect by dropping the pending response sender
    client.drop_pending_response(&request_id);

    let result = handle.await.unwrap();
    let err = result.unwrap_err();
    assert_eq!(err.code, RpcErrorCode::RecipientDisconnected as u32);
}

// =========================================================================
// v2 -> v1 tests (v2 caller, v1 handler)
// =========================================================================

/// Spec #10: Caller falls back to v1 packet when remote is v1.
#[tokio::test]
async fn test_v2_v1_caller_request_fallback() {
    let client = Arc::new(RpcClientManager::new());
    // Remote has client_protocol = 0 (v1 only)
    let transport =
        Arc::new(MockTransport::new().with_remote_protocol("dest", CLIENT_PROTOCOL_DEFAULT));

    let handle = spawn_perform_rpc(
        client.clone(),
        transport.clone(),
        PerformRpcData {
            destination_identity: "dest".into(),
            method: "greet".into(),
            payload: "hi".into(),
            response_timeout: Duration::from_secs(5),
        },
    )
    .await;

    transport.wait_for_packet().await;

    // Verify: sent as v1 packet, NOT a data stream
    assert_eq!(transport.count_packets(is_rpc_request_packet), 1);
    assert_eq!(transport.texts().iter().filter(|(_, o)| o.topic == RPC_REQUEST_TOPIC).count(), 0);

    let request_id = transport.extract_request_id();
    client.handle_ack(request_id.clone());
    client.handle_response(request_id, Some("yo".into()), None);

    let result = handle.await.unwrap();
    assert_eq!(result.unwrap(), "yo");
}

/// Spec #11: v1 handler receives v1 request and responds with v1 packet.
#[tokio::test]
async fn test_v2_v1_handler_v1_request() {
    let server = RpcServerManager::new();
    let transport = MockTransport::new();

    server.register_method("echo".to_string(), |data| Box::pin(async move { Ok(data.payload) }));

    server
        .handle_request(
            ParticipantIdentity("caller".into()),
            "req-v1".into(),
            "echo".into(),
            "v1-body".into(),
            Duration::from_secs(5),
            RPC_VERSION_V1,
            &transport,
        )
        .await;

    // ACK sent
    assert_eq!(transport.count_packets(is_rpc_ack_packet), 1);
    // Response sent as v1 packet (not data stream)
    assert_eq!(transport.count_packets(is_rpc_response_packet), 1);
    assert_eq!(transport.texts().len(), 0);

    // Verify response payload
    for p in transport.packets() {
        if let Some(proto::data_packet::Value::RpcResponse(resp)) = &p.value {
            if let Some(proto::rpc_response::Value::Payload(payload)) = &resp.value {
                assert_eq!(payload, "v1-body");
            }
        }
    }
}

/// Spec #12: Payload too large rejected for v1 remote.
#[tokio::test]
async fn test_v2_v1_payload_too_large() {
    let client = RpcClientManager::new();
    let transport = MockTransport::new().with_remote_protocol("dest", CLIENT_PROTOCOL_DEFAULT);

    let large_payload = "x".repeat(MAX_PAYLOAD_BYTES + 1);
    let result = client
        .perform_rpc(
            PerformRpcData {
                destination_identity: "dest".into(),
                method: "big".into(),
                payload: large_payload,
                response_timeout: Duration::from_secs(5),
            },
            &transport,
        )
        .await;

    let err = result.unwrap_err();
    assert_eq!(err.code, RpcErrorCode::RequestPayloadTooLarge as u32);
}

/// Spec #13: Response timeout with v1 remote.
#[tokio::test]
async fn test_v2_v1_response_timeout() {
    let client = RpcClientManager::new();
    let transport = MockTransport::new().with_remote_protocol("dest", CLIENT_PROTOCOL_DEFAULT);

    let result = client
        .perform_rpc(
            PerformRpcData {
                destination_identity: "dest".into(),
                method: "slow".into(),
                payload: "x".into(),
                response_timeout: Duration::from_millis(50),
            },
            &transport,
        )
        .await;

    let err = result.unwrap_err();
    assert_eq!(err.code, RpcErrorCode::ConnectionTimeout as u32);
}

/// Spec #14: Error response from v1 handler.
#[tokio::test]
async fn test_v2_v1_error_response() {
    let client = Arc::new(RpcClientManager::new());
    let transport =
        Arc::new(MockTransport::new().with_remote_protocol("dest", CLIENT_PROTOCOL_DEFAULT));

    let handle = spawn_perform_rpc(
        client.clone(),
        transport.clone(),
        PerformRpcData {
            destination_identity: "dest".into(),
            method: "err".into(),
            payload: "x".into(),
            response_timeout: Duration::from_secs(5),
        },
    )
    .await;

    transport.wait_for_packet().await;
    let request_id = transport.extract_request_id();

    client.handle_ack(request_id.clone());
    client.handle_response(
        request_id,
        None,
        Some(proto::RpcError { code: 101, message: "v1-err".into(), data: String::new() }),
    );

    let result = handle.await.unwrap();
    let err = result.unwrap_err();
    assert_eq!(err.code, 101);
    assert_eq!(err.message, "v1-err");
}

/// Spec #15: Participant disconnection with v1 remote.
#[tokio::test]
async fn test_v2_v1_participant_disconnection() {
    let client = Arc::new(RpcClientManager::new());
    let transport =
        Arc::new(MockTransport::new().with_remote_protocol("dest", CLIENT_PROTOCOL_DEFAULT));

    let handle = spawn_perform_rpc(
        client.clone(),
        transport.clone(),
        PerformRpcData {
            destination_identity: "dest".into(),
            method: "dc".into(),
            payload: "x".into(),
            response_timeout: Duration::from_secs(5),
        },
    )
    .await;

    transport.wait_for_packet().await;
    let request_id = transport.extract_request_id();

    client.handle_ack(request_id.clone());
    // Simulate disconnect by dropping the pending response sender
    client.drop_pending_response(&request_id);

    let result = handle.await.unwrap();
    let err = result.unwrap_err();
    assert_eq!(err.code, RpcErrorCode::RecipientDisconnected as u32);
}

// =========================================================================
// v1 -> v2 tests (v1 caller, v2 handler)
// =========================================================================

/// Spec #16: v2 handler responds with v1 packet when request was v1.
#[tokio::test]
async fn test_v1_v2_handler_response_fallback() {
    let server = RpcServerManager::new();
    let transport = MockTransport::new();

    server.register_method("echo".to_string(), |data| Box::pin(async move { Ok(data.payload) }));

    // v1 caller sends a v1 packet request to our v2 handler
    server
        .handle_request(
            ParticipantIdentity("v1-caller".into()),
            "req-v1-to-v2".into(),
            "echo".into(),
            "hello-from-v1".into(),
            Duration::from_secs(5),
            RPC_VERSION_V1,
            &transport,
        )
        .await;

    // ACK via v1 packet
    assert_eq!(transport.count_packets(is_rpc_ack_packet), 1);
    // Response via v1 packet (not data stream), even though handler supports v2
    assert_eq!(transport.count_packets(is_rpc_response_packet), 1);
    assert_eq!(transport.texts().len(), 0);
}

/// Spec #17: Unhandled error in v2 handler for v1 caller — APPLICATION_ERROR.
#[tokio::test]
async fn test_v1_v2_handler_unhandled_error() {
    let server = RpcServerManager::new();
    let transport = MockTransport::new();

    server.register_method("crash".to_string(), |_data| {
        Box::pin(async move {
            panic!("boom");
        })
    });

    server
        .handle_request(
            ParticipantIdentity("v1-caller".into()),
            "req-crash".into(),
            "crash".into(),
            "x".into(),
            Duration::from_secs(5),
            RPC_VERSION_V1,
            &transport,
        )
        .await;

    let err = extract_response_error(&transport).unwrap();
    assert_eq!(err.code, RpcErrorCode::ApplicationError as u32);
}

/// Spec #18: RpcError passthrough in v2 handler for v1 caller.
#[tokio::test]
async fn test_v1_v2_handler_rpc_error_passthrough() {
    let server = RpcServerManager::new();
    let transport = MockTransport::new();

    server.register_method("fail".to_string(), |_data| {
        Box::pin(async move { Err(RpcError::new(101, "custom-err".into(), Some("extra".into()))) })
    });

    server
        .handle_request(
            ParticipantIdentity("v1-caller".into()),
            "req-fail".into(),
            "fail".into(),
            "x".into(),
            Duration::from_secs(5),
            1,
            &transport,
        )
        .await;

    let err = extract_response_error(&transport).unwrap();
    assert_eq!(err.code, 101);
    assert_eq!(err.message, "custom-err");
}

// =========================================================================
// Additional tests
// =========================================================================

/// Verify handle_response_stream resolves the pending caller correctly.
#[tokio::test]
async fn test_v2_response_stream_resolves_caller() {
    let client = RpcClientManager::new();

    // Manually register a pending response
    let (tx, rx) = tokio::sync::oneshot::channel();
    client.insert_pending_response("req-stream".to_string(), tx);

    let reader =
        make_text_reader("stream-result", v2_response_attrs("req-stream"), RPC_RESPONSE_TOPIC);

    client.handle_response_stream(reader).await;

    let result: Result<String, RpcError> = rx.await.unwrap();
    assert_eq!(result.unwrap(), "stream-result");
}

/// Verify unregistered method returns UNSUPPORTED_METHOD error via v2 path.
#[tokio::test]
async fn test_v2_handler_unsupported_method() {
    let server = RpcServerManager::new();
    let transport = MockTransport::new();

    let reader = make_text_reader(
        "payload",
        v2_request_attrs("req-unsup", "nonexistent", 5000),
        RPC_REQUEST_TOPIC,
    );

    server.handle_request_stream(reader, ParticipantIdentity("caller".into()), &transport).await;

    let err = extract_response_error(&transport).unwrap();
    assert_eq!(err.code, RpcErrorCode::UnsupportedMethod as u32);
}
