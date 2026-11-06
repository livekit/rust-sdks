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

/// reqwest's default policy (at most 10 redirects), except that a redirect to another scheme is
/// returned instead of followed: reqwest drops `Authorization` only when the host or port
/// changes, so an `https` to `http` redirect on port 443 would send a credential in plaintext.
fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let from = attempt.previous().last().map(url::Url::scheme);
        if from.is_some_and(|scheme| scheme != attempt.url().scheme()) {
            attempt.stop()
        } else {
            reqwest::redirect::Policy::default().redirect(attempt)
        }
    })
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
        net::TcpListener,
        time::Duration,
    };

    use super::*;

    /// A redirect that changes only the scheme, on the same port, comes back as the 3xx: the
    /// request with its `Authorization` header is not sent again under the new scheme.
    #[tokio::test]
    async fn a_redirect_to_another_scheme_is_not_followed() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("the first request");
            let _ = stream.read(&mut [0; 4096]);
            let answer = format!(
                "HTTP/1.1 307 Temporary Redirect\r\nLocation: https://127.0.0.1:{port}/v1/logs\r\n\
                 Content-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(answer.as_bytes()).expect("answer");
            drop(stream);
            listener.set_nonblocking(true).expect("non-blocking");
            std::thread::sleep(Duration::from_millis(300));
            listener.accept().is_ok()
        });
        let headers = vec![Header { name: "Authorization".into(), value: "Bearer token".into() }];
        let url = format!("http://127.0.0.1:{port}/v1/logs");
        let response = NativeTransport::new()
            .request(HttpMethod::Post, url, headers, Some(b"batch".to_vec()))
            .await
            .expect("the redirect itself comes back");
        assert_eq!(response.status, 307);
        assert!(!server.join().expect("server"), "nothing was sent to the new scheme");
    }
}
