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

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

/// Render a URL for logging with secrets stripped: userinfo (`user:password@`)
/// and the query string (which can carry an access token). Keeps scheme, host,
/// port, and path.
pub fn redact_url(url: &url::Url) -> String {
    let mut u = url.clone();
    let _ = u.set_username("");
    let _ = u.set_password(None);
    u.set_query(None);
    u.set_fragment(None);
    u.to_string()
}

static WS: OnceLock<Arc<dyn WsClient>> = OnceLock::new();
static HTTP: OnceLock<Arc<dyn HttpClient>> = OnceLock::new();

/// Register the process-wide WebSocket client. Call once at startup, before the
/// first `connect`. A later call is ignored (first registration wins).
///
/// Independent of [`set_http_client`]: a consumer that only needs HTTP (e.g. a
/// token source) can register that alone, and vice versa.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn set_ws_client(c: Arc<dyn WsClient>) {
    let _ = WS.set(c);
}

/// Register the process-wide HTTP client. Call once at startup, before the first
/// request. A later call is ignored (first registration wins).
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn set_http_client(c: Arc<dyn HttpClient>) {
    let _ = HTTP.set(c);
}

/// Self-test: GET `url` via the registered HTTP client; returns the full response
/// (status + headers + body) so callers can assert the whole struct round-trips the FFI.
/// Errors if no client is registered or the transport fails.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub async fn self_test_http_get(url: String) -> Result<HttpResponse, TransportError> {
    let c =
        http_client().ok_or_else(|| TransportError::Other("no http client registered".into()))?;
    c.request(HttpMethod::Get, url, Vec::new(), None).await
}

/// Self-test: connect, send `payload`, receive one frame, close; return the echoed bytes.
/// Errors if no client is registered, the transport fails, or the peer closes first.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub async fn self_test_ws_echo(url: String, payload: Vec<u8>) -> Result<Vec<u8>, TransportError> {
    let c = ws_client().ok_or_else(|| TransportError::Other("no ws client registered".into()))?;
    let conn = c.connect(url, Vec::new(), 5_000).await?.connection;
    conn.send(payload).await?;
    let got = conn.recv().await?.ok_or(TransportError::Closed)?;
    conn.close().await;
    Ok(got)
}

/// Test probe: whether a host has explicitly registered an HTTP client via
/// [`set_http_client`].
///
/// Reports registration, not resolvability: on native builds [`http_client`]
/// still yields the built-in client when this returns `false`.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn has_http_client() -> bool {
    HTTP.get().is_some()
}

/// Test probe: whether a host has explicitly registered a WebSocket client via
/// [`set_ws_client`].
///
/// Reports registration, not resolvability: on native builds [`ws_client`] still
/// yields the built-in client when this returns `false`.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn has_ws_client() -> bool {
    WS.get().is_some()
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
        Arc::new(crate::native::NativeTransport::new())
    }
    /// A fresh native HTTP client for tests (bypasses the global registry).
    pub fn native_http_client() -> Arc<dyn HttpClient> {
        Arc::new(crate::native::NativeTransport::new())
    }
}

#[cfg(test)]
mod tests {
    use super::redact_url;

    #[test]
    fn redact_url_strips_proxy_credentials() {
        let u = url::Url::parse("http://user:s3cret@proxy.example.com:8080/path").unwrap();
        let out = redact_url(&u);
        assert_eq!(out, "http://proxy.example.com:8080/path");
        assert!(!out.contains("s3cret") && !out.contains("user"));
    }

    #[test]
    fn redact_url_strips_query_token() {
        let u = url::Url::parse(
            "wss://sfu.example.com/rtc/v1?access_token=abc.def.ghi&join_request=CAES",
        )
        .unwrap();
        let out = redact_url(&u);
        assert_eq!(out, "wss://sfu.example.com/rtc/v1");
        assert!(!out.contains("abc.def.ghi") && !out.contains("CAES"));
    }
}
