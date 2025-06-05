// Copyright 2023 LiveKit, Inc.
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

use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use livekit_protocol as proto;
use livekit_runtime::{JoinHandle, TcpStream};
use prost::Message as ProtoMessage;
use std::{env, io};

use tokio::sync::{mpsc, oneshot};

#[cfg(feature = "signal-client-tokio")]
use base64;

#[cfg(feature = "signal-client-tokio")]
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream as TokioTcpStream,
};

#[cfg(feature = "signal-client-tokio")]
use tokio_tungstenite::{
    connect_async,
    tungstenite::error::ProtocolError,
    tungstenite::{Error as WsError, Message},
    MaybeTlsStream, WebSocketStream,
};

#[cfg(feature = "__signal-client-async-compatible")]
use async_tungstenite::{
    async_std::connect_async,
    async_std::ClientStream as MaybeTlsStream,
    tungstenite::error::ProtocolError,
    tungstenite::{Error as WsError, Message},
    WebSocketStream,
};

#[cfg(feature = "signal-client-tokio")]
use std::sync::Arc;

#[cfg(feature = "signal-client-tokio")]
use tokio_rustls::rustls::{self, Certificate, RootCertStore, ClientConfig};

#[cfg(feature = "signal-client-tokio")]
const MY_ROOT_CA_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIIFpzCCA4+gAwIBAgIUBZ9icmUbGUpKyXRsigBbkWkll14wDQYJKoZIhvcNAQEL
BQAwYzELMAkGA1UEBhMCS1IxDjAMBgNVBAgMBVNlb3VsMQ4wDAYDVQQHDAVTZW91
bDEaMBgGA1UECgwRVklSTkVDVCBDTy4sIExURC4xGDAWBgNVBAMMD1Zpcm5lY3Qg
Um9vdCBDQTAeFw0yNTA1MjgwMTA2MzhaFw0zNTA1MjYwMTA2MzhaMGMxCzAJBgNV
BAYTAktSMQ4wDAYDVQQIDAVTZW91bDEOMAwGA1UEBwwFU2VvdWwxGjAYBgNVBAoM
EVZJUk5FQ1QgQ08uLCBMVEQuMRgwFgYDVQQDDA9WaXJuZWN0IFJvb3QgQ0EwggIi
MA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoICAQCx3KJgBo/P3NSr6mSEDUVx1R+W
gLug7BsGRefKfDTk1voJ/RxKBWzIQk13ClXI01sgjol1ktLE1xvpXrWNX+K3sgU0
IDzUXVSjzW7CoInlb9RvhO1MJmH9Qf8MSqhw0L5KYQgnG7/6VhUut+zBmrIcnZEG
d2+PPox7aR4Ks19em49CWjTPG5NnkbusQ/5Ce3Vga7xgya8RO4oztB55RB/pt73b
wHs2WxFp0xgwiMPHcravmvT02axCOW9APZv6zLKwr9Xqls8er9n2khrgVIWzG1ro
ZKSciSxWrkye7fTBL0g6itrP3810Efvffhrg8PI7XIQNFtjGKm8d0DAUxyk65ngg
aJHnUwZG1Na9VBhNLdb4xuZY9FKzocd+Itxe1eQq4aob6jBaG5qqgMmHDaNyRYoV
RWU4gGAtyXFcyoz5dyHdNpdSCKSKn4FTYMz8cJ/LQw7eiBbKrzWLBl8ooJnirW2c
PdX6rnj/v6HrRCS4JCgZi/6UeMf2mgzVdIWp9TMch7NWtePkA1yw+kBWoQ1g+aw6
P16X5gtRKz1UnpRU+fsRP4N6GFwC5F2zPXJawWekKd1hiFopfsrVIsO2JaueNa5J
s6hA76zOLLSk0exeJMdSvygicClfeqX4efXxvduKgdkitD4AF7dNTusUqejAVWEp
Zauu2Wxs7/vVr4EGlQIDAQABo1MwUTAdBgNVHQ4EFgQUYnu9q3XEczGr6goLUIS3
jj0MdBUwHwYDVR0jBBgwFoAUYnu9q3XEczGr6goLUIS3jj0MdBUwDwYDVR0TAQH/
BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAgEAIoflMKGkKBYEda+SDqJGGwh98X1T
QY7slnzFqRAiPpAGall6gDwJmIExsDKuOb/RhJn2ApHUh16kEO8p1Xk4aSln4k8e
y+Kz5d+lbfxKw6SW6znXxYHZjSI/PP4LJhz1XoJ7qwV8MBbhnXNXP/o4pWbYPtEk
r08OxA7gbavYyWYyGA/mu59E71/IfRr4eRr86FAcwMOiD19ausVCJufaTX0eTzBj
A0kJIkqsuTNoZZL1eGotQ1O6fa+ZTNyDHZFfBE4bGlLyO4ZyEC82XoHC3oc06GOb
mIjO1WkaPYxEe9JZYQi7FC6H2A1aBjJAlObE8AeYKjXr7AjGQwpVCJe1zx/+0Ng/
I8HU2T63Tj5G24YXNuGtCRL2RwBrcSQ6QLhBeKlWU/tt+GGVXJ8jGtjgeXsshGJ1
MYaKYQNlREfk59IAA4MIt8AshbGDTuqtDZyQ1+yqpjoQvO5ZZM6HYvyA/+uu6SaF
HKwen0WDks+FS8MnKxnMKpmWPvHCxU4WV0SfBxPGq5PbO4SeSn7xCXAmrY3c7oc+
98o+DM2Ds3ZIBQ85F/bPj/GpQVGJ/uht5XZUj+TsDQeVrPApEc/Wx1YNQyI6T/+R
RY2qqMdsVeQUeY8+t4ntangrYj+oM1dS72pfYTsbZEwCgzHEeO4l6bOqBuqcDqCx
SZ1QIlhgMRu5jDY=
-----END CERTIFICATE-----"#;

use super::{SignalError, SignalResult};

type WebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug)]
enum InternalMessage {
    Signal {
        signal: proto::signal_request::Message,
        response_chn: oneshot::Sender<SignalResult<()>>,
    },
    Pong {
        ping_data: Vec<u8>,
    },
    Close,
}

/// SignalStream hold the WebSocket connection
///
/// It is replaced by [SignalClient] at each reconnection.
#[derive(Debug)]
pub(super) struct SignalStream {
    internal_tx: mpsc::Sender<InternalMessage>,
    read_handle: JoinHandle<()>,
    write_handle: JoinHandle<()>,
}

impl SignalStream {
    /// Connect to livekit websocket.
    /// Return SignalError if the connections failed
    ///
    /// SignalStream will never try to reconnect if the connection has been
    /// closed.
    pub async fn connect(
        url: url::Url,
    ) -> SignalResult<(Self, mpsc::UnboundedReceiver<Box<proto::signal_response::Message>>)> {
        {
            // Don't log sensitive info
            let mut url = url.clone();
            let filtered_pairs: Vec<_> = url
                .query_pairs()
                .filter(|(key, _)| key != "access_token")
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();

            {
                let mut query_pairs = url.query_pairs_mut();
                query_pairs.clear();
                for (key, value) in filtered_pairs {
                    query_pairs.append_pair(&key, &value);
                }

                query_pairs.append_pair("access_token", "...");
            }

            log::info!("connecting to {}", url);
        }

        #[cfg(feature = "signal-client-tokio")]
        let ws_stream = {
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
                        WsError::Io(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("Invalid proxy URL: {}", e),
                        ))
                    })?;

                    let host = url.host_str().ok_or_else(|| {
                        WsError::Io(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Target URL has no host",
                        ))
                    })?;

                    let port = url.port_or_known_default().ok_or_else(|| {
                        WsError::Io(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Target URL has no port and no default for scheme",
                        ))
                    })?;

                    let proxy_host = proxy_url.host_str().ok_or_else(|| {
                        WsError::Io(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Proxy URL has no host",
                        ))
                    })?;

                    let proxy_port = proxy_url.port_or_known_default().unwrap_or(80);
                    let proxy_addr = format!("{}:{}", proxy_host, proxy_port);

                    let mut proxy_stream =
                        TokioTcpStream::connect(proxy_addr).await.map_err(WsError::Io)?;

                    let mut proxy_auth_header = None;
                    if let Some(password) = proxy_url.password() {
                        let auth = format!("{}:{}", proxy_url.username(), password);
                        let auth = format!("Basic {}", base64::encode(auth));
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
                    proxy_stream.write_all(connect_req.as_bytes()).await.map_err(WsError::Io)?;

                    // Read and parse response
                    let mut response = Vec::new();
                    let mut buf = [0u8; 4096];
                    let mut headers_complete = false;

                    while !headers_complete {
                        let n = proxy_stream.read(&mut buf).await.map_err(WsError::Io)?;
                        if n == 0 {
                            return Err(WsError::Io(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "Proxy connection closed while reading response",
                            ))
                            .into());
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
                        WsError::Io(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Invalid proxy response",
                        ))
                    })?;

                    // Check status code
                    if !status_line.contains("200") {
                        return Err(WsError::Io(io::Error::new(
                            io::ErrorKind::ConnectionRefused,
                            format!("Proxy connection failed: {}", status_line),
                        ))
                        .into());
                    }

                    log::debug!("Proxy connection established to {}", target);

                    // Create MaybeTlsStream based on original URL scheme
                    let stream = if url.scheme() == "wss" {
                        // Only enable proxy TLS support when rustls-tls-native-roots is enabled
                        #[cfg(feature = "rustls-tls-native-roots")]
                        {
                            // For WSS, we need to establish TLS over the proxy connection
                            use std::sync::Arc;
                            use tokio_rustls::{rustls, TlsConnector};

                            // Load native root certificates
                            let mut root_store = rustls::RootCertStore::empty();
                            match rustls_native_certs::load_native_certs() {
                                Ok(certs) => {
                                    let roots: Vec<rustls::Certificate> = certs
                                        .into_iter()
                                        .map(|cert| rustls::Certificate(cert.0))
                                        .collect();

                                    for root in roots {
                                        root_store.add(&root).map_err(|e| {
                                            WsError::Io(io::Error::new(
                                                io::ErrorKind::Other,
                                                format!(
                                                    "Failed to parse root certificate: {:?}",
                                                    e
                                                ),
                                            ))
                                        })?;
                                    }
                                }
                                Err(e) => {
                                    return Err(WsError::Io(io::Error::new(
                                        io::ErrorKind::Other,
                                        format!("Could not load native root certificates: {}", e),
                                    ))
                                    .into());
                                }
                            }

                            let tls_config = rustls::ClientConfig::builder()
                                .with_safe_defaults()
                                .with_root_certificates(root_store)
                                .with_no_client_auth();

                            let server_name = rustls::ServerName::try_from(host).map_err(|_| {
                                WsError::Io(io::Error::new(
                                    io::ErrorKind::InvalidInput,
                                    format!("Invalid DNS name: {}", host),
                                ))
                            })?;

                            let connector = TlsConnector::from(Arc::new(tls_config));
                            let tls_stream = connector
                                .connect(server_name, proxy_stream)
                                .await
                                .map_err(|e| {
                                    WsError::Io(io::Error::new(
                                        io::ErrorKind::Other,
                                        format!("TLS connection error: {}", e),
                                    ))
                                })?;

                            MaybeTlsStream::Rustls(tls_stream)
                        }

                        #[cfg(not(feature = "rustls-tls-native-roots"))]
                        {
                            // For non-rustls-tls-native-roots builds, don't support proxy for WSS
                            return Err(WsError::Io(io::Error::new(
                                io::ErrorKind::Other,
                                "WSS over proxy requires rustls-tls-native-roots feature",
                            ))
                            .into());
                        }
                    } else {
                        // For plain WS, just use the proxy stream directly
                        MaybeTlsStream::Plain(proxy_stream)
                    };

                    // Now perform WebSocket handshake over the established connection
                    let (ws_stream, _) =
                        tokio_tungstenite::client_async_with_config(url, stream, None).await?;
                    ws_stream
                } else {
                    // No proxy specified, connect directly
                    let (ws_stream, _) = connect_async(url).await?;
                    ws_stream
                }
            } else {
                // Non-tokio build or no proxy - connect directly
                let (ws_stream, _) = connect_async(url).await?;
                ws_stream
            };

            ws_stream
        };

        #[cfg(not(feature = "signal-client-tokio"))]
        let (ws_stream, _) = connect_async(url).await?;
        let (ws_writer, ws_reader) = ws_stream.split();

        let (emitter, events) = mpsc::unbounded_channel();
        let (internal_tx, internal_rx) = mpsc::channel::<InternalMessage>(8);
        let write_handle = livekit_runtime::spawn(Self::write_task(internal_rx, ws_writer));
        let read_handle =
            livekit_runtime::spawn(Self::read_task(internal_tx.clone(), ws_reader, emitter));

        Ok((Self { internal_tx, read_handle, write_handle }, events))
    }

    /// Close the websocket
    /// It sends a CloseFrame to the server before closing
    pub async fn close(self, notify_close: bool) {
        if notify_close {
            let _ = self.internal_tx.send(InternalMessage::Close).await;
        }
        let _ = self.write_handle.await;
        let _ = self.read_handle.await;
    }

    /// Send a SignalRequest to the websocket
    /// It also waits for the message to be sent
    pub async fn send(&self, signal: proto::signal_request::Message) -> SignalResult<()> {
        let (send, recv) = oneshot::channel();
        let msg = InternalMessage::Signal { signal, response_chn: send };
        let _ = self.internal_tx.send(msg).await;
        recv.await.map_err(|_| SignalError::SendError)?
    }

    /// This task is used to send messages to the websocket
    /// It is also responsible for closing the connection
    async fn write_task(
        mut internal_rx: mpsc::Receiver<InternalMessage>,
        mut ws_writer: SplitSink<WebSocket, Message>,
    ) {
        while let Some(msg) = internal_rx.recv().await {
            match msg {
                InternalMessage::Signal { signal, response_chn } => {
                    let data = proto::SignalRequest { message: Some(signal) }.encode_to_vec();

                    if let Err(err) = ws_writer.send(Message::Binary(data)).await {
                        let _ = response_chn.send(Err(err.into()));
                        break;
                    }

                    let _ = response_chn.send(Ok(()));
                }
                InternalMessage::Pong { ping_data } => {
                    if let Err(err) = ws_writer.send(Message::Pong(ping_data)).await {
                        log::error!("failed to send pong message: {:?}", err);
                    }
                }
                InternalMessage::Close => break,
            }
        }

        let _ = ws_writer.close().await;
    }

    /// This task is used to read incoming messages from the websocket
    /// and dispatch them through the EventEmitter.
    ///
    /// It can also send messages to [handle_write] task ( Used e.g. answer to pings )
    async fn read_task(
        internal_tx: mpsc::Sender<InternalMessage>,
        mut ws_reader: SplitStream<WebSocket>,
        emitter: mpsc::UnboundedSender<Box<proto::signal_response::Message>>,
    ) {
        while let Some(msg) = ws_reader.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    let res = proto::SignalResponse::decode(data.as_slice())
                        .expect("failed to decode SignalResponse");

                    if let Some(msg) = res.message {
                        let _ = emitter.send(Box::new(msg));
                    }
                }
                Ok(Message::Ping(data)) => {
                    let _ = internal_tx.send(InternalMessage::Pong { ping_data: data }).await;
                    continue;
                }
                Ok(Message::Close(close)) => {
                    log::debug!("server closed the connection: {:?}", close);
                    break;
                }
                Ok(Message::Frame(_)) => {}
                Err(WsError::Protocol(ProtocolError::ResetWithoutClosingHandshake)) => {
                    break; // Ignore
                }
                _ => {
                    log::error!("unhandled websocket message {:?}", msg);
                    break;
                }
            }
        }

        let _ = internal_tx.send(InternalMessage::Close).await;
    }
}
