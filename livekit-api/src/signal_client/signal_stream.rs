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

use tokio::sync::{mpsc, oneshot};

#[cfg(feature = "signal-client-tokio")]
use tokio_tungstenite::{
    connect_async,
    tungstenite::error::ProtocolError,
    tungstenite::{Error as WsError, Message},
    MaybeTlsStream, WebSocketStream,
    Connector,
};

#[cfg(feature = "signal-client-tokio")]
use std::sync::Arc;

#[cfg(feature = "signal-client-tokio")]
use tokio_rustls::rustls::{self, RootCertStore, ClientConfig};
#[cfg(feature = "signal-client-tokio")]
use rustls::pki_types::CertificateDer;

#[cfg(feature = "signal-client-tokio")]
const MY_ROOT_CA_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIIFpzCCA4+gAwIBAgIUXfmnFm/ufGwuoqKf9QNw9xDE5lAwDQYJKoZIhvcNAQEL
BQAwYzELMAkGA1UEBhMCS1IxDjAMBgNVBAgMBVNlb3VsMQ4wDAYDVQQHDAVTZW91
bDEaMBgGA1UECgwRVklSTkVDVCBDTy4sIExURC4xGDAWBgNVBAMMD1Zpcm5lY3Qg
Um9vdCBDQTAeFw0yNTA2MDIwMjUxNThaFw0zNTA1MzEwMjUxNThaMGMxCzAJBgNV
BAYTAktSMQ4wDAYDVQQIDAVTZW91bDEOMAwGA1UEBwwFU2VvdWwxGjAYBgNVBAoM
EVZJUk5FQ1QgQ08uLCBMVEQuMRgwFgYDVQQDDA9WaXJuZWN0IFJvb3QgQ0EwggIi
MA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoICAQCNHuepYYD2K8dYUB9jlv8JUVa6
PGxIT8itE5pVeWHT+Wpc2UdChW4AOOEOnrQv+tJRi+bseyu/BtJ5czfBGLkXgLtz
8WAAe5ALplvaVmfogoiuKQjNy9KCVngq/vfZgaR4YI3m4c/jgb1TKPm4vP1uQdrH
DdwUHg8iPGyjeEX+3x2MvFcAESIdPIpuwKtNb+irTjc+QeBGVA3B2O9zbggtOyTg
D8UyTWsFIplAI+w1Tt6ovCKUErcyMSsycEB8G6KW6Zv/mrWbppWOnxJExJc23jC7
nt/M3ENvZyKP0HDf643T7EIJ+SAYtIszVVEAaYjWFxVh36ItdWyXzJO9GO3XgKXa
7DxGEI74tCQ24UlUOwGa1INW/WwX3uEvVco0xid+iTdnur5Q8UXTcFUSjnR7WNTc
2M3kXOBUrN97OQ70o2EOQfHW+YyOSBBpLwJyKLPQNE00laGNeP06UZnWtQGRHg4e
3M/i4XaPcQfjwIcbsySxTH+lbNwNCfz9ISLJPKOSZR2ESlAEV5f/VjAQhSVYHmNt
kbIvHS3ajHEaXHQTfrXP/xaSvoRswq0ZQLosYNB8dezLnbzvsrX7qvI7+iH6BFya
hczyAPRe5MqMSofaWAeIzNzA+71M+xmzPAFevmTntV89cTE4C1CyR9ucr4hGLfbh
bgQzU95YM4WAZhCKvwIDAQABo1MwUTAdBgNVHQ4EFgQU4BGuqQLCMi47i0+jPYU4
oZUeD8AwHwYDVR0jBBgwFoAU4BGuqQLCMi47i0+jPYU4oZUeD8AwDwYDVR0TAQH/
BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAgEAiBM87aqUZqsgcP+n4CfAsxRb4sSr
yKzCcW8kYJtx6Tj9vT66pUePvYMdM/OGav395/NB8X8kemX4mQNgApliRBszsJ2i
hC2ZL+Vf0jyFB1OFHH1CocdALCG8l4M6em740JDSWajnlUH3caXohvh7atm7VXpf
VNG1VIHMBiugfB1TvGzjcLUlgVTzOW7sd3JwbAeeVlsv2Uc6gpxWKs9frfTGxSKL
jmEjOPQb1Tp1cBQsM+DcMxDTlh2uGEApwzwjWxPOuGT5womRw1vUQJxkEKgPnCDI
Xs3m3d24O2hZdbOkWZ9dNxH0Q65Ne3idURzGg/Lbd4iNz9pp/ZTebyJ8U572W+x2
IRKyQ+nYPgvzQanJyFAELOfWbdpwwRO0zm0dAwYSbzBpIJjTFDC/xJMvHrLPxTLZ
zR3tAZ+BPJhsmq0LwZlXqrVBF0U4pm6iJ4hOMTg4iNcuhxfo3g32eZhjK3kKKdFQ
V6yTzvSy0zIkC15g67AXV+G60AAw4XJ440y2sjaQAMRdN6a7milEcQyBlAmkMSm2
w9ESbMVC3IkLhTp21XNDhEKG8GPOaTYPIl1tpIUJD0RtBjAb1Uh87UB9y/Nuuee5
lSL8F3goKRewe0xQAqokpW3DWhfwlkKaInSSn9xTE/E+jFeXmvPi1eP7tkdDDD4f
XEl6MIZPi8V2QYY=
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
            if url.scheme() == "wss" {
                let mut root_store = RootCertStore::empty();
                let mut pem = MY_ROOT_CA_PEM.as_bytes();
                let certs: Vec<_> = rustls_pemfile::certs(&mut pem)
                    .collect();
                for cert in certs {
                    let cert = cert.map_err(|_| SignalError::SendError)?;
                    root_store.add(cert).map_err(|_| SignalError::SendError)?;
                }
                let config = ClientConfig::builder()
                    .with_root_certificates(root_store)
                    .with_no_client_auth();
                let connector = Connector::Rustls(Arc::new(config));
                let (ws_stream, _) = tokio_tungstenite::connect_async_tls_with_config(url, None, false, Some(connector)).await?;
                ws_stream
            } else {
                let (ws_stream, _) = connect_async(url).await?;
                ws_stream
            }
        };

        #[cfg(not(feature = "signal-client-tokio"))]
        let ws_stream = {
            let (ws_stream, _) = connect_async(url).await?;
            ws_stream
        };

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