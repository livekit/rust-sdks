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

use crate::types::{Header, HttpResponse, TransportError};
use std::sync::Arc;

/// A single open WebSocket connection. Control frames (ping/pong, close handshake)
/// are the implementation's own responsibility; only binary application frames cross here.
#[async_trait::async_trait]
pub trait WsConnection: Send + Sync + 'static {
    /// Send one binary application frame.
    async fn send(&self, frame: Vec<u8>) -> Result<(), TransportError>;
    /// Receive the next binary frame. `Ok(None)` means the peer/transport closed.
    async fn recv(&self) -> Result<Option<Vec<u8>>, TransportError>;
    /// Close the connection, sending a close frame if the transport supports it.
    async fn close(&self);
}

/// The connection opened by [`WsClient::connect`].
///
/// A record wrapper, not a bare `Arc<dyn WsConnection>`: uniffi 0.31 cannot
/// lift a trait object returned from an async `with_foreign` method.
pub struct WsConnectResult {
    pub connection: Arc<dyn WsConnection>,
}

/// A host- or Rust-provided WebSocket transport. Opens the LiveKit signalling
/// WebSocket; knows nothing about LiveKit/protobuf.
#[async_trait::async_trait]
pub trait WsClient: Send + Sync {
    /// Open a WebSocket. `url` is the full ws(s):// URL including query string.
    /// `timeout_ms` bounds connection establishment. Milliseconds, not `Duration`:
    /// a `Duration` parameter breaks uniffi-dart's foreign codegen.
    async fn connect(
        &self,
        url: String,
        headers: Vec<Header>,
        timeout_ms: u64,
    ) -> Result<WsConnectResult, TransportError>;
}

/// The HTTP method for an [`HttpClient::request`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

/// A host- or Rust-provided HTTP transport. Performs the request/response HTTP
/// calls LiveKit needs (connection validation, region discovery, token endpoints);
/// knows nothing about LiveKit/protobuf.
///
/// Implementors provide the single [`request`](HttpClient::request) primitive;
/// [`HttpClientExt`] layers `get`/`post` on top, so adding verbs never widens the
/// implementation (or, for foreign impls, the FFI) surface.
#[async_trait::async_trait]
pub trait HttpClient: Send + Sync {
    /// Perform one HTTP request, sending `body` if present.
    async fn request(
        &self,
        method: HttpMethod,
        url: String,
        headers: Vec<Header>,
        body: Option<Vec<u8>>,
    ) -> Result<HttpResponse, TransportError>;
}

/// `get`/`post` conveniences over [`HttpClient::request`], blanket-implemented for
/// every [`HttpClient`] (including `dyn HttpClient`). Not part of the `HttpClient`
/// vtable, so verbs added here never reach implementors.
#[async_trait::async_trait]
pub trait HttpClientExt: HttpClient {
    /// Perform a GET.
    async fn get(
        &self,
        url: String,
        headers: Vec<Header>,
    ) -> Result<HttpResponse, TransportError> {
        self.request(HttpMethod::Get, url, headers, None).await
    }

    /// Perform a POST with the given body.
    async fn post(
        &self,
        url: String,
        headers: Vec<Header>,
        body: Vec<u8>,
    ) -> Result<HttpResponse, TransportError> {
        self.request(HttpMethod::Post, url, headers, Some(body)).await
    }
}

#[async_trait::async_trait]
impl<T: HttpClient + ?Sized> HttpClientExt for T {}
