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

mod transport;
mod types;

#[cfg(feature = "__native")]
mod native;

pub use transport::{
    HttpClient, HttpClientExt, HttpMethod, WsClient, WsConnectResult, WsConnection,
};
pub use types::{Header, HttpResponse, TransportError};

use std::sync::{Arc, OnceLock};

static WS: OnceLock<Arc<dyn WsClient>> = OnceLock::new();
static HTTP: OnceLock<Arc<dyn HttpClient>> = OnceLock::new();

/// Register the process-wide WebSocket client. Call once at startup, before the
/// first `connect`. A later call is ignored (first registration wins).
///
/// Independent of [`set_http_client`]: a consumer that only needs HTTP (e.g. a
/// token source) can register that alone, and vice versa.
pub fn set_ws_client(c: Arc<dyn WsClient>) {
    let _ = WS.set(c);
}

/// Register the process-wide HTTP client. Call once at startup, before the first
/// request. A later call is ignored (first registration wins).
pub fn set_http_client(c: Arc<dyn HttpClient>) {
    let _ = HTTP.set(c);
}

/// Resolve the process-wide WebSocket client.
///
/// Returns the explicitly registered client if any; otherwise, on native builds,
/// the built-in native client; otherwise `None`.
pub fn ws_client() -> Option<Arc<dyn WsClient>> {
    if let Some(c) = WS.get() {
        return Some(Arc::clone(c));
    }
    #[cfg(feature = "__native")]
    {
        Some(native::native_ws_client())
    }
    #[cfg(not(feature = "__native"))]
    {
        None
    }
}

/// Resolve the process-wide HTTP client.
///
/// Returns the explicitly registered client if any; otherwise, on native builds,
/// the built-in native client; otherwise `None`.
pub fn http_client() -> Option<Arc<dyn HttpClient>> {
    if let Some(c) = HTTP.get() {
        return Some(Arc::clone(c));
    }
    #[cfg(feature = "__native")]
    {
        Some(native::native_http_client())
    }
    #[cfg(not(feature = "__native"))]
    {
        None
    }
}

#[cfg(feature = "__native")]
pub mod testing {
    use crate::{HttpClient, WsClient};
    use std::sync::Arc;
    /// A fresh native WebSocket client for tests (bypasses the global registry).
    pub fn native_ws_client() -> Arc<dyn WsClient> {
        Arc::new(crate::native::NativeTransport)
    }
    /// A fresh native HTTP client for tests (bypasses the global registry).
    pub fn native_http_client() -> Arc<dyn HttpClient> {
        Arc::new(crate::native::NativeTransport)
    }
}
