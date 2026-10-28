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

//! Shared mock transport for livekit-api unit tests.
//!
//! `install_mock_transport()` registers the mock exactly once (process-global
//! OnceLock), so it is safe to call from multiple tests in the same binary.
//! All tests share the same deterministic mock.
//!
//! MockConn::recv() yields one canned `proto::SignalResponse { Pong }` frame
//! and then returns `Ok(None)`.
//!
//! MockTransport::connect to a URL containing `silent` instead yields a SilentConn,
//! which stays open and never sends a frame.
//!
//! MockTransport::request dispatches deterministically on the request inputs:
//!
//! 1. If the request has NO `Authorization: Bearer <non-empty>` header →
//!    returns 401. This means any test that expects `Ok` from `validate()` or
//!    `fetch_from_endpoint()` implicitly proves the Bearer token was forwarded.
//! 2. Else dispatches on the URL path:
//!    - path contains `/settings/regions` → 200 + canned RegionUrlResponse JSON
//!    - URL contains the marker `connrefused` → `TransportError::Connection`
//!      (exercises the `error_with_chain` mapping in `fetch_from_endpoint`)
//!    - URL contains the marker `badjson` → 200 + non-JSON body
//!      (exercises the serde parse-error path)
//!    - otherwise (validate or any other endpoint) → 200 + empty body

use livekit_net::{
    Header, HttpMethod, HttpResponse, TransportError, WsClient, WsConnectResult, WsConnection,
};
use livekit_protocol as proto;
use prost::Message as _;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex, Once,
};
use tokio::sync::Mutex as AsyncMutex;

/// Every URL the mock's HTTP client was asked for. Tests share it, so assert on a host unique
/// to the test.
pub static HTTP_REQUESTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

// ---------------------------------------------------------------------------
// MockConn: yields one canned Pong frame then None
// ---------------------------------------------------------------------------

pub struct MockConn {
    outbound: AsyncMutex<Vec<Vec<u8>>>,
}

#[async_trait::async_trait]
impl WsConnection for MockConn {
    async fn send(&self, _frame: Vec<u8>) -> Result<(), TransportError> {
        Ok(())
    }

    async fn recv(&self) -> Result<Option<Vec<u8>>, TransportError> {
        Ok(self.outbound.lock().await.pop())
    }

    async fn close(&self) {}
}

/// Open connections to a URL containing `silent`. Only one test dials that marker, so the
/// count is not shared with concurrent tests.
pub static SILENT_CONNS: AtomicUsize = AtomicUsize::new(0);

/// Stays open and never answers, so a join or reconnect response never arrives.
struct SilentConn;

impl SilentConn {
    fn new() -> Self {
        SILENT_CONNS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for SilentConn {
    fn drop(&mut self) {
        SILENT_CONNS.fetch_sub(1, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl WsConnection for SilentConn {
    async fn send(&self, _frame: Vec<u8>) -> Result<(), TransportError> {
        Ok(())
    }

    async fn recv(&self) -> Result<Option<Vec<u8>>, TransportError> {
        std::future::pending().await
    }

    async fn close(&self) {}
}

// ---------------------------------------------------------------------------
// MockTransport: creates a MockConn pre-loaded with one Pong frame
// ---------------------------------------------------------------------------

pub struct MockTransport;

#[async_trait::async_trait]
impl WsClient for MockTransport {
    async fn connect(
        &self,
        url: String,
        _headers: Vec<Header>,
        _timeout_ms: u64,
    ) -> Result<WsConnectResult, TransportError> {
        if url.contains("silent") {
            return Ok(WsConnectResult { connection: Arc::new(SilentConn::new()) });
        }
        let pong = proto::SignalResponse {
            message: Some(proto::signal_response::Message::PongResp(proto::Pong::default())),
        };
        // vec is popped from the end, so push the last frame first
        let frames = vec![pong.encode_to_vec()];
        Ok(WsConnectResult { connection: Arc::new(MockConn { outbound: AsyncMutex::new(frames) }) })
    }
}

#[async_trait::async_trait]
impl livekit_net::HttpClient for MockTransport {
    async fn request(
        &self,
        _method: HttpMethod,
        url: String,
        headers: Vec<Header>,
        _body: Option<Vec<u8>>,
    ) -> Result<HttpResponse, TransportError> {
        HTTP_REQUESTS.lock().unwrap().push(url.clone());

        // Rule 1: require a non-empty Bearer token.
        // Any test that expects Ok() from validate() or fetch_from_endpoint()
        // therefore implicitly proves the Bearer header was forwarded.
        let has_bearer = headers.iter().any(|h| {
            h.name.eq_ignore_ascii_case("Authorization")
                && h.value.starts_with("Bearer ")
                && h.value.len() > "Bearer ".len()
        });
        if !has_bearer {
            return Ok(HttpResponse {
                status: 401,
                headers: vec![],
                body: b"missing bearer".to_vec(),
            });
        }

        // Rule 2: dispatch on URL.
        if url.contains("connrefused") {
            // Simulate a connection-refused transport error to exercise error_with_chain.
            return Err(TransportError::Connection(
                "error trying to connect: connection refused".into(),
            ));
        }
        if url.contains("badjson") {
            // Return non-JSON body to exercise the serde parse-error path.
            return Ok(HttpResponse {
                status: 200,
                headers: vec![],
                body: b"this is not json".to_vec(),
            });
        }
        if url.contains("/settings/regions") {
            // Serve a canned RegionUrlResponse JSON for region discovery tests.
            let body = br#"{"regions":[{"region":"us-mock-1","url":"wss://us-mock.livekit.cloud","distance":"10"}]}"#;
            return Ok(HttpResponse { status: 200, headers: vec![], body: body.to_vec() });
        }
        // Default: validate or any other endpoint — return 200 with empty body.
        Ok(HttpResponse { status: 200, headers: vec![], body: vec![] })
    }
}

// ---------------------------------------------------------------------------
// Once-guarded registration
// ---------------------------------------------------------------------------

static INSTALL: Once = Once::new();

/// Register the shared MockTransport exactly once.
/// Safe to call from multiple tests — subsequent calls are no-ops.
pub fn install_mock_transport() {
    INSTALL.call_once(|| {
        livekit_net::set_ws_client(Arc::new(MockTransport));
        livekit_net::set_http_client(Arc::new(MockTransport));
    });
}
