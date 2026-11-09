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

/// An opt-in view over the shared transport; no back-reference to the view is retained.
struct LimitedNativeClient {
    transport: Arc<NativeTransport>,
    limit: u64,
}

#[async_trait::async_trait]
impl HttpClient for LimitedNativeClient {
    async fn request(
        &self,
        method: HttpMethod,
        url: String,
        headers: Vec<Header>,
        body: Option<Vec<u8>>,
    ) -> Result<HttpResponse, TransportError> {
        self.transport.request_inner(method, &url, &headers, body, Some(self.limit)).await
    }
}

pub(crate) fn native_http_client_with_response_limit(limit: u64) -> Arc<dyn HttpClient> {
    Arc::new(LimitedNativeClient { transport: default_transport(), limit })
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
        self.request_inner(method, &url, &headers, body, None).await
    }
}

impl NativeTransport {
    async fn request_inner(
        &self,
        method: HttpMethod,
        url: &str,
        headers: &[Header],
        body: Option<Vec<u8>>,
        limit: Option<u64>,
    ) -> Result<HttpResponse, TransportError> {
        let mut req = match method {
            HttpMethod::Get => self.http.get(url),
            HttpMethod::Post => self.http.post(url),
        };
        for h in headers {
            req = req.header(&h.name, &h.value);
        }
        if let Some(body) = body {
            req = req.body(body);
        }
        let mut res = req.send().await.map_err(|e| TransportError::Connection(error_chain(&e)))?;
        let status = res.status().as_u16();
        let headers: Vec<_> = res
            .headers()
            .iter()
            .filter_map(|(n, v)| {
                v.to_str().ok().map(|v| Header { name: n.as_str().to_owned(), value: v.to_owned() })
            })
            .collect();
        let Some(limit) = limit else {
            let body =
                res.bytes().await.map_err(|e| TransportError::Other(e.to_string()))?.to_vec();
            return Ok(HttpResponse { status, headers, body });
        };
        let mut body = crate::bounded::BodyReader::new(status, headers, limit)?;
        while let Some(chunk) =
            res.chunk().await.map_err(|e| TransportError::Other(e.to_string()))?
        {
            body.push(&chunk)?;
        }
        body.finish()
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

    #[tokio::test]
    async fn bounded_responses_count_decoded_bytes_and_leave_shared_requests_uncapped() {
        use flate2::{write::GzEncoder, Compression};
        let cap = 65_536;
        for size in [cap - 1, cap, cap + 1] {
            for framing in [
                "length",
                "chunked",
                "close",
                "false-length",
                "gzip",
                "gzip-chunked",
                "gzip-corrupt",
                "gzip-multi",
            ] {
                let mut decoded = vec![b'x'; size];
                // A disabled phrase beyond the accepted prefix must never classify a partial body.
                if size > cap {
                    let phrase = b"data recording is disabled by owner";
                    decoded[size - phrase.len()..].copy_from_slice(phrase);
                }
                let mut wire = if framing.starts_with("gzip") {
                    let mut zip = GzEncoder::new(Vec::new(), Compression::default());
                    if framing == "gzip-multi" {
                        zip.write_all(&decoded[..decoded.len() / 2]).expect("gzip first member");
                        let mut first = zip.finish().expect("first member");
                        let mut second = GzEncoder::new(Vec::new(), Compression::default());
                        second
                            .write_all(&decoded[decoded.len() / 2..])
                            .expect("gzip second member");
                        first.extend(second.finish().expect("second member"));
                        first
                    } else {
                        zip.write_all(&decoded).expect("gzip");
                        zip.finish().expect("finish")
                    }
                } else {
                    decoded.clone()
                };
                if framing == "gzip-corrupt" {
                    let last = wire.len() - 1;
                    wire[last] ^= 1;
                }
                let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
                let addr = listener.local_addr().expect("address");
                let server = std::thread::spawn(move || {
                    let (mut stream, _) = listener.accept().expect("accept");
                    stream.set_read_timeout(Some(Duration::from_secs(5))).expect("timeout");
                    read_request(&mut stream);
                    let framing_header = match framing {
                        "chunked" | "gzip-chunked" => "Transfer-Encoding: chunked\r\n".into(),
                        "close" => String::new(),
                        "false-length" => format!("Content-Length: {}\r\n", wire.len() + 1),
                        _ => format!("Content-Length: {}\r\n", wire.len()),
                    };
                    let encoding =
                        if framing.starts_with("gzip") { "Content-Encoding: gzip\r\n" } else { "" };
                    let head = format!("HTTP/1.1 403 Forbidden\r\nRetry-After: 17\r\n{framing_header}{encoding}Connection: close\r\n\r\n");
                    stream.write_all(head.as_bytes()).expect("headers");
                    if matches!(framing, "chunked" | "gzip-chunked") {
                        for piece in wire.chunks(8192) {
                            if write!(stream, "{:x}\r\n", piece.len())
                                .and_then(|_| stream.write_all(piece))
                                .and_then(|_| stream.write_all(b"\r\n"))
                                .is_err()
                            {
                                return;
                            }
                        }
                        let _ = stream.write_all(b"0\r\n\r\n");
                    } else {
                        let _ = stream.write_all(&wire);
                    }
                });
                let client = NativeTransport::new();
                let result = client
                    .request_inner(
                        HttpMethod::Get,
                        &format!("http://{addr}"),
                        &[],
                        None,
                        Some(cap as u64),
                    )
                    .await;
                drop(client);
                // Release a failed setup's blocked accept before joining its thread.
                let _ = TcpStream::connect_timeout(&addr, Duration::from_millis(100));
                let server_joined = server.join().is_ok();
                assert!(server_joined, "server joined before response assertions");
                if size > cap {
                    assert!(
                        matches!(result, Err(TransportError::ResponseTooLarge { ref headers })
                        if headers.iter().any(|h| h.name.eq_ignore_ascii_case("retry-after") && h.value == "17")),
                        "{size} {framing}: {result:?}"
                    );
                } else if matches!(framing, "false-length" | "gzip-corrupt") {
                    assert!(result.is_err(), "truncated framing must fail");
                } else {
                    assert_eq!(result.expect(framing).body.len(), size);
                }
            }
        }
        // The default/shared request path still returns the complete body, including gzip bytes.
        for gzip in [false, true] {
            let decoded = vec![b'x'; cap * 2];
            let wire = if gzip {
                let mut zip = GzEncoder::new(Vec::new(), Compression::default());
                zip.write_all(&decoded).expect("gzip");
                zip.finish().expect("finish")
            } else {
                decoded
            };
            let expected = wire.clone();
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let addr = listener.local_addr().expect("address");
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept");
                read_request(&mut stream);
                let encoding = if gzip { "Content-Encoding: gzip\r\n" } else { "" };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\n{encoding}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    wire.len()
                )
                .expect("header");
                stream.write_all(&wire).expect("body");
            });
            let result = NativeTransport::new()
                .request(HttpMethod::Get, format!("http://{addr}"), vec![], None)
                .await;
            let _ = TcpStream::connect_timeout(&addr, Duration::from_millis(100));
            let server_joined = server.join().is_ok();
            assert!(server_joined, "server joined before response assertions");
            assert_eq!(result.expect("shared request").body, expected);
        }
    }

    #[tokio::test]
    async fn bounded_reader_finishes_before_the_rest_of_the_body_and_cancellation_destroys_owner() {
        for cancel in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let addr = listener.local_addr().expect("address");
            let (release, wait) = std::sync::mpsc::sync_channel(1);
            let (ready, arrived) = tokio::sync::oneshot::channel();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept");
                stream.set_read_timeout(Some(Duration::from_secs(5))).expect("read timeout");
                stream.set_write_timeout(Some(Duration::from_secs(5))).expect("write timeout");
                read_request(&mut stream);
                stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 10000000\r\nRetry-After: 17\r\n\r\n").expect("headers");
                if !cancel {
                    stream.write_all(&vec![b'x'; 65_537]).expect("prefix");
                }
                let _ = ready.send(());
                let released = wait.recv_timeout(Duration::from_secs(10)).is_ok();
                // The client must finish/abort before the server releases the remaining body.
                released
            });
            let client = Arc::new(NativeTransport::new());
            let weak = Arc::downgrade(&client);
            let task = tokio::spawn(async move {
                client
                    .request_inner(
                        HttpMethod::Get,
                        &format!("http://{addr}"),
                        &[],
                        None,
                        Some(65_536),
                    )
                    .await
            });
            let readiness = tokio::time::timeout(Duration::from_secs(5), arrived).await;
            let (result, joined) = if cancel {
                task.abort();
                (None, task.await.is_err_and(|e| e.is_cancelled()))
            } else {
                let mut task = task;
                let answer = tokio::time::timeout(Duration::from_secs(5), &mut task).await;
                match answer {
                    Ok(Ok(result)) => (Some(result), true),
                    _ => {
                        task.abort();
                        let _ = task.await;
                        (None, false)
                    }
                }
            };
            let _ = release.send(());
            let _ = TcpStream::connect_timeout(&addr, Duration::from_millis(100));
            let released = server.join().unwrap_or(false);
            assert!(
                matches!(readiness, Ok(Ok(()))) && joined && released && weak.upgrade().is_none()
            );
            if !cancel {
                assert!(matches!(result, Some(Err(TransportError::ResponseTooLarge { .. }))));
            }
        }
    }

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
    /// Needs a TLS backend: without one the client cannot attempt an `https` request at all.
    #[cfg(any(
        feature = "native-tls",
        feature = "native-tls-vendored",
        feature = "rustls-tls-native-roots",
        feature = "rustls-tls-webpki-roots"
    ))]
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
