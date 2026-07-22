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

#[cfg(feature = "__native-tokio")]
use crate::TransportError;

#[cfg(feature = "__native-tokio")]
use std::env;

/// Map a tungstenite error from a WS handshake to a [`TransportError`].
///
/// When the server returns an HTTP error response during the upgrade (e.g. 404
/// on the /rtc endpoint), tungstenite surfaces it as `Error::Http(response)`.
/// We extract the status code and return `TransportError::Http { status }` so
/// callers can distinguish HTTP error codes (403, 404) from network errors.
#[cfg(feature = "__native-tokio")]
fn map_ws_err(e: tokio_tungstenite::tungstenite::Error) -> TransportError {
    use tokio_tungstenite::tungstenite::Error;
    match e {
        Error::Http(resp) => TransportError::Http { status: resp.status().as_u16() },
        other => TransportError::Connection(other.to_string()),
    }
}

#[cfg(feature = "__native-tokio")]
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream as TokioTcpStream,
};

#[cfg(feature = "__native-tokio")]
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

#[cfg(feature = "__native-tokio")]
use livekit_runtime::TcpStream;

#[cfg(feature = "__native-tokio")]
pub(super) async fn connect_ws(
    request: http::Request<()>,
    url: &url::Url,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, TransportError> {
    // Check for HTTP_PROXY or HTTPS_PROXY environment variables
    let proxy_env = if url.scheme() == "wss" {
        env::var("HTTPS_PROXY").or_else(|_| env::var("https_proxy"))
    } else {
        env::var("HTTP_PROXY").or_else(|_| env::var("http_proxy"))
    };

    // Connect directly or through proxy
    let ws_stream = if let Ok(proxy_url) = proxy_env {
        if !proxy_url.is_empty() {
            log::info!("Using proxy: {}", proxy_url);
            let proxy_url = url::Url::parse(&proxy_url).map_err(|e| {
                TransportError::Connection(format!("Invalid proxy URL: {}", e))
            })?;

            let host = url.host_str().ok_or_else(|| {
                TransportError::Connection("Target URL has no host".into())
            })?;

            let port = url.port_or_known_default().ok_or_else(|| {
                TransportError::Connection(
                    "Target URL has no port and no default for scheme".into(),
                )
            })?;

            let proxy_host = proxy_url.host_str().ok_or_else(|| {
                TransportError::Connection("Proxy URL has no host".into())
            })?;

            let proxy_port = proxy_url.port_or_known_default().unwrap_or(80);
            let proxy_addr = format!("{}:{}", proxy_host, proxy_port);

            let mut proxy_stream = TokioTcpStream::connect(proxy_addr)
                .await
                .map_err(|e| TransportError::Connection(e.to_string()))?;

            let mut proxy_auth_header = None;
            if let Some(password) = proxy_url.password() {
                use base64::Engine as _;
                let auth = format!("{}:{}", proxy_url.username(), password);
                let auth = format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD.encode(auth)
                );
                proxy_auth_header = Some(auth);
            }

            // Send CONNECT request
            let target = format!("{}:{}", host, port);
            let mut connect_req =
                format!("CONNECT {} HTTP/1.1\r\nHost: {}\r\n", target, target);

            // Add proxy authorization if needed
            if let Some(auth) = proxy_auth_header {
                connect_req.push_str(&format!("Proxy-Authorization: {}\r\n", auth));
            }

            // Finalize request
            connect_req.push_str("\r\n");

            log::debug!("Sending CONNECT request to proxy");
            proxy_stream
                .write_all(connect_req.as_bytes())
                .await
                .map_err(|e| TransportError::Connection(e.to_string()))?;

            // Read and parse response
            let mut response = Vec::new();
            let mut buf = [0u8; 4096];
            let mut headers_complete = false;

            while !headers_complete {
                let n = proxy_stream
                    .read(&mut buf)
                    .await
                    .map_err(|e| TransportError::Connection(e.to_string()))?;
                if n == 0 {
                    return Err(TransportError::Connection(
                        "Proxy connection closed while reading response".into(),
                    ));
                }

                response.extend_from_slice(&buf[..n]);

                // Check if we've received the end of headers (double CRLF)
                if response.windows(4).any(|w| w == b"\r\n\r\n") {
                    headers_complete = true;
                }
            }

            // Parse status line
            let response_str = String::from_utf8_lossy(&response);
            let status_line = response_str.lines().next().ok_or_else(|| {
                TransportError::Connection("Invalid proxy response".into())
            })?;

            // Check status code
            if !status_line.contains("200") {
                return Err(TransportError::Connection(format!(
                    "Proxy connection failed: {}",
                    status_line
                )));
            }

            log::debug!("Proxy connection established to {}", target);

            // Create MaybeTlsStream based on original URL scheme
            let stream = if url.scheme() == "wss" {
                // Only enable proxy TLS support when rustls-tls-native-roots is enabled
                #[cfg(feature = "rustls-tls-native-roots")]
                {
                    // For WSS, we need to establish TLS over the proxy connection
                    use std::sync::Arc;
                    use tokio_rustls::{
                        rustls::{self, pki_types::ServerName},
                        TlsConnector,
                    };

                    // Load native root certificates
                    let mut root_store = rustls::RootCertStore::empty();
                    let cert_result = rustls_native_certs::load_native_certs();
                    if !cert_result.errors.is_empty() {
                        log::warn!(
                            "Native root CA certificate loading errors: {:?}",
                            cert_result.errors
                        );
                    }
                    if cert_result.certs.is_empty() {
                        return Err(TransportError::Connection(format!(
                            "Could not load any native root certificates: {:?}",
                            cert_result.errors
                        )));
                    }
                    let total = cert_result.certs.len();
                    let (added, ignored) =
                        root_store.add_parsable_certificates(cert_result.certs);
                    log::debug!(
                        "Added {added}/{total} native root certificates ({ignored} ignored)"
                    );

                    let tls_config = rustls::ClientConfig::builder()
                        .with_root_certificates(root_store)
                        .with_no_client_auth();

                    let server_name =
                        ServerName::try_from(host.to_owned()).map_err(|_| {
                            TransportError::Connection(format!(
                                "Invalid DNS name: {}",
                                host
                            ))
                        })?;

                    let connector = TlsConnector::from(Arc::new(tls_config));
                    let tls_stream = connector
                        .connect(server_name, proxy_stream)
                        .await
                        .map_err(|e| {
                            TransportError::Connection(format!(
                                "TLS connection error: {}",
                                e
                            ))
                        })?;

                    MaybeTlsStream::Rustls(tls_stream)
                }

                #[cfg(not(feature = "rustls-tls-native-roots"))]
                {
                    // For non-rustls-tls-native-roots builds, don't support proxy for WSS
                    return Err(TransportError::Connection(
                        "WSS over proxy requires rustls-tls-native-roots feature".into(),
                    ));
                }
            } else {
                // For plain WS, just use the proxy stream directly
                MaybeTlsStream::Plain(proxy_stream)
            };

            // Now perform WebSocket handshake over the established connection
            let (ws_stream, _) =
                tokio_tungstenite::client_async_with_config(request, stream, None)
                    .await
                    .map_err(map_ws_err)?;
            ws_stream
        } else {
            // Proxy var is empty, connect directly
            let (ws_stream, _) = connect_async(request)
                .await
                .map_err(map_ws_err)?;
            ws_stream
        }
    } else {
        // No proxy env var set, connect directly
        let (ws_stream, _) = connect_async(request)
            .await
            .map_err(map_ws_err)?;
        ws_stream
    };

    Ok(ws_stream)
}
