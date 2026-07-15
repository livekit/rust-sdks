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

mod connection;
mod proxy;

use crate::{Header, HttpResponse, PlatformConnectResult, PlatformTransport, TransportError};
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

pub struct NativeTransport;

pub(crate) fn native_default() -> Arc<dyn PlatformTransport> {
    use std::sync::OnceLock;
    static DEFAULT: OnceLock<Arc<NativeTransport>> = OnceLock::new();
    DEFAULT.get_or_init(|| Arc::new(NativeTransport)).clone()
}

#[async_trait::async_trait]
impl PlatformTransport for NativeTransport {
    async fn connect(
        &self,
        url: String,
        headers: Vec<Header>,
        timeout_ms: u64,
    ) -> Result<PlatformConnectResult, TransportError> {
        let parsed =
            url::Url::parse(&url).map_err(|e| TransportError::Connection(e.to_string()))?;

        #[cfg(feature = "__native-tokio")]
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        #[cfg(feature = "__native-async")]
        use async_tungstenite::tungstenite::client::IntoClientRequest;

        let mut request = parsed
            .clone()
            .into_client_request()
            .map_err(|e| TransportError::Connection(e.to_string()))?;
        for h in &headers {
            let name: http::header::HeaderName = h
                .name
                .parse()
                .map_err(|_| TransportError::Other("bad header name".into()))?;
            let value = http::header::HeaderValue::from_str(&h.value)
                .map_err(|_| TransportError::Other("bad header value".into()))?;
            request.headers_mut().insert(name, value);
        }

        let connect_fut = async {
            #[cfg(feature = "__native-tokio")]
            let ws = self::proxy::connect_ws(request, &parsed).await?;
            #[cfg(feature = "__native-async")]
            let (ws, _) = async_tungstenite::async_std::connect_async(request)
                .await
                .map_err(|e: async_tungstenite::tungstenite::Error| {
                    use async_tungstenite::tungstenite::Error;
                    match e {
                        Error::Http(resp) => TransportError::Http { status: resp.status().as_u16() },
                        other => TransportError::Connection(other.to_string()),
                    }
                })?;
            Ok::<_, TransportError>(ws)
        };

        let ws = livekit_runtime::timeout(
            std::time::Duration::from_millis(timeout_ms),
            connect_fut,
        )
        .await
        .map_err(|_| TransportError::Timeout)??;

        Ok(PlatformConnectResult {
            connection: Arc::new(self::connection::NativeConnection::new(ws)),
        })
    }

    async fn http_get(
        &self,
        url: String,
        headers: Vec<Header>,
    ) -> Result<HttpResponse, TransportError> {
        #[cfg(feature = "__native-tokio")]
        {
            let mut req = reqwest::Client::new().get(&url);
            for h in &headers {
                req = req.header(&h.name, &h.value);
            }
            let res = req
                .send()
                .await
                .map_err(|e| TransportError::Connection(error_chain(&e)))?;
            let status = res.status().as_u16();
            let resp_headers = res
                .headers()
                .iter()
                .filter_map(|(n, v)| {
                    v.to_str().ok().map(|v| Header { name: n.as_str().to_string(), value: v.to_string() })
                })
                .collect();
            let body = res
                .bytes()
                .await
                .map_err(|e| TransportError::Other(e.to_string()))?
                .to_vec();
            Ok(HttpResponse { status, headers: resp_headers, body })
        }
        #[cfg(feature = "__native-async")]
        {
            let mut builder = isahc::Request::get(&url);
            for h in &headers {
                builder = builder.header(h.name.as_str(), h.value.as_str());
            }
            let request = builder
                .body(())
                .map_err(|e| TransportError::Other(e.to_string()))?;
            let mut res = isahc::send_async(request)
                .await
                .map_err(|e| TransportError::Connection(error_chain(&e)))?;
            let status = res.status().as_u16();
            let resp_headers = res
                .headers()
                .iter()
                .filter_map(|(n, v)| {
                    v.to_str().ok().map(|v| Header { name: n.as_str().to_string(), value: v.to_string() })
                })
                .collect();
            use isahc::AsyncReadResponseExt;
            let mut body = Vec::new();
            res.copy_to(&mut body)
                .await
                .map_err(|e| TransportError::Other(e.to_string()))?;
            Ok(HttpResponse { status, headers: resp_headers, body })
        }
    }
}
