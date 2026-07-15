// Copyright 2025 LiveKit, Inc.
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

use crate::types::{Header, HttpResponse, TransportError};
use std::sync::Arc;

/// A single open WebSocket connection. Control frames (ping/pong, close handshake)
/// are the implementation's own responsibility; only binary application frames cross here.
#[async_trait::async_trait]
pub trait PlatformConnection: Send + Sync + 'static {
    /// Send one binary application frame.
    async fn send(&self, frame: Vec<u8>) -> Result<(), TransportError>;
    /// Receive the next binary frame. `Ok(None)` means the peer/transport closed.
    async fn recv(&self) -> Result<Option<Vec<u8>>, TransportError>;
    async fn close(&self);
}

/// The connection opened by [`PlatformTransport::connect`].
///
/// A record wrapper, not a bare `Arc<dyn PlatformConnection>`: uniffi 0.31 cannot
/// lift a trait object returned from an async `with_foreign` method.
pub struct PlatformConnectResult {
    pub connection: Arc<dyn PlatformConnection>,
}

/// A host- or Rust-provided network transport. Opens WebSockets and performs the
/// pre-WS HTTP GETs. Knows nothing about LiveKit/protobuf.
#[async_trait::async_trait]
pub trait PlatformTransport: Send + Sync {
    /// Open a WebSocket. `url` is the full ws(s):// URL including query string.
    /// `timeout_ms` bounds connection establishment. Milliseconds, not `Duration`:
    /// a `Duration` parameter breaks uniffi-dart's foreign codegen.
    async fn connect(
        &self,
        url: String,
        headers: Vec<Header>,
        timeout_ms: u64,
    ) -> Result<PlatformConnectResult, TransportError>;

    /// The pre-WS HTTP GETs (validate, region discovery).
    async fn http_get(
        &self,
        url: String,
        headers: Vec<Header>,
    ) -> Result<HttpResponse, TransportError>;
}
