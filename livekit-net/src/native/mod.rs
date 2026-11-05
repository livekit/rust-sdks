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
    Header, HttpBodyStream, HttpClient, HttpMethod, HttpResponse, HttpStreamClient,
    HttpStreamResponse, TransportError, WsClient, WsConnectResult,
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
        Self { http: reqwest::Client::new() }
    }
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

pub(crate) fn native_http_stream_client() -> Arc<dyn HttpStreamClient> {
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

impl NativeTransport {
    /// Sends the request and returns once the status and headers have arrived.
    async fn send(
        &self,
        method: HttpMethod,
        url: &str,
        headers: &[Header],
        body: Option<Vec<u8>>,
    ) -> Result<reqwest::Response, TransportError> {
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
        req.send().await.map_err(|e| TransportError::Connection(error_chain(&e)))
    }
}

fn headers_of(res: &reqwest::Response) -> Vec<Header> {
    res.headers()
        .iter()
        .filter_map(|(n, v)| {
            v.to_str().ok().map(|v| Header { name: n.as_str().to_string(), value: v.to_string() })
        })
        .collect()
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
        let res = self.send(method, &url, &headers, body).await?;
        let status = res.status().as_u16();
        let resp_headers = headers_of(&res);
        let body = res.bytes().await.map_err(|e| TransportError::Other(e.to_string()))?.to_vec();
        Ok(HttpResponse { status, headers: resp_headers, body })
    }
}

#[async_trait::async_trait]
impl HttpStreamClient for NativeTransport {
    async fn request_stream(
        &self,
        method: HttpMethod,
        url: String,
        headers: Vec<Header>,
        body: Option<Vec<u8>>,
    ) -> Result<HttpStreamResponse, TransportError> {
        let res = self.send(method, &url, &headers, body).await?;
        Ok(HttpStreamResponse {
            status: res.status().as_u16(),
            headers: headers_of(&res),
            body: Arc::new(NativeBody(tokio::sync::Mutex::new(res))),
        })
    }
}

/// A reqwest response body. Dropping it drops the response, which closes the
/// connection instead of returning it to the pool with unread data.
struct NativeBody(tokio::sync::Mutex<reqwest::Response>);

#[async_trait::async_trait]
impl HttpBodyStream for NativeBody {
    async fn recv(&self) -> Result<Option<Vec<u8>>, TransportError> {
        let chunk = self.0.lock().await.chunk().await;
        chunk.map(|c| c.map(|c| c.to_vec())).map_err(|e| TransportError::Other(error_chain(&e)))
    }
}
