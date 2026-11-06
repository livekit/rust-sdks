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

mod connection;
mod proxy;

use crate::{
    Header, HttpClient, HttpMethod, HttpResponse, TransportError, WsClient, WsConnectResult,
};
use std::sync::Arc;

fn error_chain(e: &dyn std::error::Error) -> String {
    let mut msg = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        src = s.source();
    }
    msg
}

/// The built-in native transport. One type implements both [`WsClient`] and
/// [`HttpClient`]. On the tokio backend it owns the reqwest client, so the
/// connection pool lives and dies with the transport object.
pub struct NativeTransport {
    // reqwest pools connections per client, so a per-request client would redo
    // TCP/TLS setup (and root-store loading) on every call.
    http: reqwest::Client,
}

impl NativeTransport {
    pub(crate) fn new() -> Self {
        let http = reqwest::Client::builder()
            .redirect(redirect_policy())
            .build()
            // `reqwest::Client::new` panics the same way.
            .expect("the TLS backend initializes");
        Self { http }
    }
}

/// reqwest's default policy (at most 10 redirects), except that a redirect from `https` to a
/// plaintext scheme is returned instead of followed: reqwest drops `Authorization` only when the
/// host or port changes, so an `https` to `http` redirect on port 443 would send a credential in
/// plaintext. An `http` to `https` upgrade is still followed (a token endpoint configured as
/// `http://` that redirects to `https://` keeps working).
fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let downgrade =
            attempt.previous().last().is_some_and(|from| downgrades(from, attempt.url()));
        if downgrade {
            attempt.stop()
        } else {
            reqwest::redirect::Policy::default().redirect(attempt)
        }
    })
}

/// Whether following a redirect from `from` to `to` would leave TLS.
fn downgrades(from: &url::Url, to: &url::Url) -> bool {
    from.scheme() == "https" && to.scheme() != "https"
}

/// The shared default transport behind the registry fallback. Memoized because
/// consumers resolve a client per request: a fresh transport per resolution
/// would defeat its connection pooling.
fn default_transport() -> Arc<NativeTransport> {
    use std::sync::OnceLock;
    static DEFAULT: OnceLock<Arc<NativeTransport>> = OnceLock::new();
    Arc::clone(DEFAULT.get_or_init(|| Arc::new(NativeTransport::new())))
}

pub(crate) fn native_ws_client() -> Arc<dyn WsClient> {
    default_transport()
}

pub(crate) fn native_http_client() -> Arc<dyn HttpClient> {
    default_transport()
}

#[async_trait::async_trait]
impl WsClient for NativeTransport {
    async fn connect(
        &self,
        url: String,
        headers: Vec<Header>,
        timeout_ms: u64,
    ) -> Result<WsConnectResult, TransportError> {
        let parsed =
            url::Url::parse(&url).map_err(|e| TransportError::Connection(e.to_string()))?;

        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let mut request = parsed
            .clone()
            .into_client_request()
            .map_err(|e| TransportError::Connection(e.to_string()))?;
        for h in &headers {
            let name: http::header::HeaderName =
                h.name.parse().map_err(|_| TransportError::Other("bad header name".into()))?;
            let value = http::header::HeaderValue::from_str(&h.value)
                .map_err(|_| TransportError::Other("bad header value".into()))?;
            request.headers_mut().insert(name, value);
        }

        let connect_fut = async {
            let ws = self::proxy::connect_ws(request, &parsed).await?;
            Ok::<_, TransportError>(ws)
        };

        let ws = tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), connect_fut)
            .await
            .map_err(|_| TransportError::Timeout)??;

        Ok(WsConnectResult { connection: Arc::new(self::connection::NativeConnection::new(ws)) })
    }
}

#[async_trait::async_trait]
impl HttpClient for NativeTransport {
    async fn request(
        &self,
        method: HttpMethod,
        url: String,
        headers: Vec<Header>,
        body: Option<Vec<u8>>,
    ) -> Result<HttpResponse, TransportError> {
        {
            let client = &self.http;
            let mut req = match method {
                HttpMethod::Get => client.get(&url),
                HttpMethod::Post => client.post(&url),
            };
            for h in &headers {
                req = req.header(&h.name, &h.value);
            }
            if let Some(body) = body {
                req = req.body(body);
            }
            let res = req.send().await.map_err(|e| TransportError::Connection(error_chain(&e)))?;
            let status = res.status().as_u16();
            let resp_headers = res
                .headers()
                .iter()
                .filter_map(|(n, v)| {
                    v.to_str()
                        .ok()
                        .map(|v| Header { name: n.as_str().to_string(), value: v.to_string() })
                })
                .collect();
            let body =
                res.bytes().await.map_err(|e| TransportError::Other(e.to_string()))?.to_vec();
            Ok(HttpResponse { status, headers: resp_headers, body })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        time::{Duration, Instant},
    };

    use super::*;

    #[test]
    fn only_a_redirect_out_of_tls_is_a_downgrade() {
        let url = |s: &str| url::Url::parse(s).expect(s);
        assert!(downgrades(&url("https://h:443/a"), &url("http://h:443/b")));
        assert!(downgrades(&url("https://h/a"), &url("http://h/b")));
        assert!(!downgrades(&url("http://h/a"), &url("https://h/b")), "an upgrade is followed");
        assert!(!downgrades(&url("http://h/a"), &url("http://h/b")));
        assert!(!downgrades(&url("https://h/a"), &url("https://other/b")));
    }

    /// Read one HTTP/1.1 request whole — the headers, then `Content-Length` bytes of body — so
    /// the answer never races a client that writes the body in a second packet.
    fn read_request(stream: &mut TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        let mut buf = [0; 4096];
        let header_end = loop {
            let n = stream.read(&mut buf).expect("read");
            if n == 0 {
                return request;
            }
            request.extend_from_slice(&buf[..n]);
            if let Some(at) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                break at + 4;
            }
        };
        let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
        let length: usize = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0);
        while request.len() < header_end + length {
            let n = stream.read(&mut buf).expect("read");
            if n == 0 {
                break;
            }
            request.extend_from_slice(&buf[..n]);
        }
        request
    }

    /// Whether `listener` gets a connection within `patience`.
    fn connected_within(listener: &TcpListener, patience: Duration) -> bool {
        listener.set_nonblocking(true).expect("non-blocking");
        let deadline = Instant::now() + patience;
        while Instant::now() < deadline {
            if listener.accept().is_ok() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// A redirect that upgrades `http` to `https` on the same host and port is followed: the
    /// listener sees the client come back (and fail its TLS handshake against a plain socket).
    #[tokio::test]
    async fn a_redirect_to_https_is_followed() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("the first request");
            let request = read_request(&mut stream);
            assert!(request.ends_with(b"batch"), "{}", String::from_utf8_lossy(&request));
            let answer = format!(
                "HTTP/1.1 307 Temporary Redirect\r\nLocation: https://127.0.0.1:{port}/v1/logs\r\n\
                 Content-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(answer.as_bytes()).expect("answer");
            drop(stream);
            connected_within(&listener, Duration::from_secs(5))
        });
        let headers = vec![Header { name: "Authorization".into(), value: "Bearer token".into() }];
        let url = format!("http://127.0.0.1:{port}/v1/logs");
        let result = NativeTransport::new()
            .request(HttpMethod::Post, url, headers, Some(b"batch".to_vec()))
            .await;
        assert!(server.join().expect("server"), "the upgraded request was never attempted");
        assert!(result.is_err(), "a plain socket cannot complete the TLS handshake: {result:?}");
    }
}
