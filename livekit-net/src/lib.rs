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

mod bounded;
mod transport;
mod types;

#[cfg(feature = "native")]
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
    #[cfg(feature = "native")]
    {
        Some(native::native_ws_client())
    }
    #[cfg(not(feature = "native"))]
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
    #[cfg(feature = "native")]
    {
        Some(native::native_http_client())
    }
    #[cfg(not(feature = "native"))]
    {
        None
    }
}

/// Resolve an opt-in client view with an inclusive decoded response-body byte limit.
///
/// Without a registered client, the native fallback stops reading on overflow (including gzip), returning
/// [`TransportError::ResponseTooLarge`] with the received headers. Ordinary [`http_client`]
/// requests keep their existing behavior. A registered host client is checked only after its
/// callback returns a buffered response. Gzip is decoded under the same limit, but this cannot
/// bound host read-time allocation. Headers must describe the returned body: a host that already
/// decoded gzip must remove Content-Encoding and Content-Length. The host owns read-time
/// enforcement. No callback API changes.
pub fn http_client_with_response_limit(max_response_bytes: u64) -> Option<Arc<dyn HttpClient>> {
    if let Some(client) = HTTP.get() {
        return Some(Arc::new(LimitedHostClient {
            client: Arc::clone(client),
            limit: max_response_bytes,
        }));
    }
    #[cfg(feature = "native")]
    {
        Some(native::native_http_client_with_response_limit(max_response_bytes))
    }
    #[cfg(not(feature = "native"))]
    {
        None
    }
}

/// Owns the registered client without a reverse edge; host buffering remains host-owned.
struct LimitedHostClient {
    client: Arc<dyn HttpClient>,
    limit: u64,
}

#[async_trait::async_trait]
impl HttpClient for LimitedHostClient {
    async fn request(
        &self,
        method: HttpMethod,
        url: String,
        headers: Vec<Header>,
        body: Option<Vec<u8>>,
    ) -> Result<HttpResponse, TransportError> {
        let response = self.client.request(method, url, headers, body).await?;
        if response.headers.iter().any(|h| {
            h.name.eq_ignore_ascii_case("content-encoding")
                && !h.value.trim().eq_ignore_ascii_case("identity")
        }) {
            let mut body = bounded::BodyReader::new(response.status, response.headers, self.limit)?;
            body.push(&response.body)?;
            return body.finish();
        }
        if response.body.len() as u64 > self.limit {
            return Err(TransportError::ResponseTooLarge { headers: response.headers });
        }
        Ok(response)
    }
}

#[cfg(feature = "native")]
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
    use super::*;

    struct EncodedHost(HttpResponse);

    #[async_trait::async_trait]
    impl HttpClient for EncodedHost {
        async fn request(
            &self,
            _: HttpMethod,
            _: String,
            _: Vec<Header>,
            _: Option<Vec<u8>>,
        ) -> Result<HttpResponse, TransportError> {
            Ok(self.0.clone())
        }
    }

    #[tokio::test]
    async fn buffered_host_limits_decoded_gzip_without_changing_ordinary_client() {
        use std::io::Write;
        for size in [65_535, 65_536, 65_537] {
            for compressible in [false, true] {
                let mut state = 13u64;
                let decoded: Vec<_> = (0..size)
                    .map(|_| {
                        state ^= state << 13;
                        state ^= state >> 7;
                        state ^= state << 17;
                        if compressible {
                            b'x'
                        } else {
                            state as u8
                        }
                    })
                    .collect();
                let mut zip =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
                zip.write_all(&decoded).expect("gzip");
                let encoded = zip.finish().expect("complete gzip");
                let host = Arc::new(EncodedHost(HttpResponse {
                    status: 403,
                    body: encoded.clone(),
                    headers: vec![
                        Header { name: "Content-Encoding".into(), value: "gzip".into() },
                        Header { name: "Retry-After".into(), value: "17".into() },
                    ],
                }));
                let weak = Arc::downgrade(&host);
                let limited = LimitedHostClient { client: host.clone(), limit: 65_536 };
                let result =
                    limited.request(HttpMethod::Get, "http://test".into(), vec![], None).await;
                let ordinary =
                    host.request(HttpMethod::Get, "http://test".into(), vec![], None).await;
                drop((limited, host));
                assert!(weak.upgrade().is_none(), "host view released before assertions");
                assert_eq!(ordinary.expect("uncapped").body, encoded);
                if size > 65_536 {
                    assert!(matches!(result, Err(TransportError::ResponseTooLarge { ref headers })
                    if headers.iter().any(|h| h.name == "Retry-After" && h.value == "17")));
                } else {
                    assert_eq!(result.expect("decoded host response below cap").body, decoded);
                }
            }
        }
    }

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
