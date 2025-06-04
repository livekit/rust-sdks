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
use tokio_rustls::rustls::{self, Certificate, RootCertStore, ClientConfig};use tokio_rustls::rustls::{self, RootCertStore, ClientConfig};
#[cfg(feature = "signal-client-tokio")]
use rustls::pki_types::CertificateDer;
#[cfg(feature = "signal-client-tokio")]
const MY_ROOT_CA_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIIEujCCAqKgAwIBAgIUGPmGvrXP3M7Duidx10zdkjPOxDMwDQYJKoZIhvcNAQEL
BQAwaDELMAkGA1UEBhMCS1IxDjAMBgNVBAgMBVNlb3VsMQ4wDAYDVQQHDAVTZW91
bDEaMBgGA1UECgwRVklSTkVDVCBDTy4sIExURC4xHTAbBgNVBAMMFFZpcm5lY3Qg
UlNBIDQwOTYgVjAxMB4XDTI1MDYwMjAyNTE1OVoXDTM1MDUzMTAyNTE1OVowXzEL
MAkGA1UEBhMCS1IxDjAMBgNVBAgMBVNlb3VsMQ4wDAYDVQQHDAVTZW91bDEaMBgG
A1UECgwRVklSTkVDVCBDTy4sIExURC4xFDASBgNVBAMMC3Zpcm5lY3QuY29tMIIB
IjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEApk64Bt0Ex0PIcRQmFQd2oD42
Xs0C7lnmLM5CDMPlzKrlhlsMnDK2zuGpGBwbs+akniCkfRPs5ZcCKMHi9z9kBIGi
4gkJSOoWpWdrhnjysu8R6BQagV9XdJcjhdir21lQNmV9u/BkzVm8/xuOxdze8z1E
+pLOFwWmF3dtZoJFIY2fBT2mTpPdkbldkCGgobk+zIvHCpZejPX5Ui4xkdM/pK6K
n7R8M4pOE+biTWSm0mcsL4dNp3I8c/UsYl6Eearx9aisASCg9SUb68DGALLHmoAk
R2oCJ6EncTe34EM1Lm8ufkQ3JbQyke4SiVgpziZ1Klfq/pZNdpUOCZJ2v1Mb7wID
AQABo2UwYzAhBgNVHREEGjAYhwQKyArJhwQKyArMhwQKyArKhwQKyArQMB0GA1Ud
DgQWBBSLYYJiO2On5GlBrWUzNj4FG3xdmzAfBgNVHSMEGDAWgBTOzkADngTlWgYU
a7oMdO/D4RqpezANBgkqhkiG9w0BAQsFAAOCAgEAFAZZ7Vkp2po9Kadd5XCjrwBf
vHpRC776tL7mabke69BZMQDZmFZzhx00TX8Jd7r29pRKVNoIyuuT7kNKPWsS7C/9
+P/0vFr9soPR2WGshrowfZqElDeKDiY5d4wDdt2gHuxJ856M/HZuu/NlHjYbdHoe
CsNkBB6vCG/aQVCUqos+ulcouzkneuVrFWlmbfYqwL20qWyFrbn0ktDwwPD00ljp
DMHPWQq0GiiYwQn/2TS9YaHF0O2picPQhbS4Lnh6ZgNkRuqWoLti9F4meo7bd/MF
zf/Br8evDpdqOtFljCY8j4ikP9jETIO8mobF6H7BwDSOcmCVsOeRLlfwflMQ4/9a
HNWfFHQFdGnD+7Ayc//zaH8UTTnZp9hnkdlIBUshepW9gBQU6OAUdfOZutMq/Pjb
IIgW+0SSKaf0E7XghyLJ6Z+qFkvSOUQ9x0tEbLRKT2aaEyLug3sRURNtXxV9oyi8
WBvfwphdZk+5LRBflV1yVcLBOZErXE4OYi0pTJ1VACwbZGscFpIZ3Me5sKFZ+RE7
JFjh4oiOX4Y75OhN+np9fRC4tAHhmsdnS5DSYmiEtdQlVsWqOuoXpTe3DBDPhWLm
h0vAsZpk2SG5grzCgHBBoISZ9gj0ey/YuKXS7Sz5R1oBQNxDLUsWZvRZzwbe3/TX
fa6gMcIisOPF+i7P5sg=
-----END CERTIFICATE-----
-----BEGIN CERTIFICATE-----
MIIF5DCCA8ygAwIBAgIUL3ytpWHHgVcBYX7JzetXMFnviVAwDQYJKoZIhvcNAQEL
BQAwYzELMAkGA1UEBhMCS1IxDjAMBgNVBAgMBVNlb3VsMQ4wDAYDVQQHDAVTZW91
bDEaMBgGA1UECgwRVklSTkVDVCBDTy4sIExURC4xGDAWBgNVBAMMD1Zpcm5lY3Qg
Um9vdCBDQTAeFw0yNTA2MDIwMjUxNTlaFw0zNTA1MzEwMjUxNTlaMGgxCzAJBgNV
BAYTAktSMQ4wDAYDVQQIDAVTZW91bDEOMAwGA1UEBwwFU2VvdWwxGjAYBgNVBAoM
EVZJUk5FQ1QgQ08uLCBMVEQuMR0wGwYDVQQDDBRWaXJuZWN0IFJTQSA0MDk2IFYw
MTCCAiIwDQYJKoZIhvcNAQEBBQADggIPADCCAgoCggIBAI31ziR2xLzFWyfTzjub
6YsOH/svhInVQyVcqgms3Atftblj3T45Tq3PHRbbFZW1C2uxAVHDcjUzaKVueZ2t
4bQh+J7MTP50ZyYLiBPq5rLlPj/zxARrym7cPOViXsMhWaHUM5YfGXKxVaM52lJo
5iSIB/a2p/NS2Hs581lJBmA0HAKvj7D01qMhTFocXsaIy+p96rqeNf4VQPzy+GYT
1A12c3WAPOlFs37x/fyBeA6YE9CpqOm7eySG3P638Vca1K5x4IPqegFok8sNDRHN
6ZeF+Q2FYFOZVpOEXWY10I08Bmsgm94OE+GwEvCrYagW1MWsvPkLbMwVH5MNs39a
tz4mUEg/ebbp7FWuWgEce0ntsaU4EMBySEV1fqE+uYMHhNoN/hPY2Xa6iaby1llU
ax6o9g9Ge1D4XHmDpfZ406IC+FoXSlyGq4laFHhee3HHVNnehnm3wkGPGv+2MJxC
Ye/gYbrGifvUG+wrsDQ1Ak5Gcr1319g2MjDu7OgbmhxV0OupmTee1+IbqHsIRO/z
BKKoEJSg45dsNJTPVopr942jynLdXEE7VnspGeUNmdKwRnR7lpx5bkbb3j7BnYA5
Ft/XEK0EMbWF7CKBCxE+qiN9So6UJVhN/ETtcZ+nn8Z/T+m7IuJd2Dt93cvpbwgH
FO7DlcyTZYKRwNlAMY1EoTgZAgMBAAGjgYowgYcwIQYDVR0RBBowGIcECsgKyYcE
CsgKzIcECsgKyocECsgK0DASBgNVHRMBAf8ECDAGAQH/AgEAMA4GA1UdDwEB/wQE
AwIBhjAdBgNVHQ4EFgQUzs5AA54E5VoGFGu6DHTvw+EaqXswHwYDVR0jBBgwFoAU
4BGuqQLCMi47i0+jPYU4oZUeD8AwDQYJKoZIhvcNAQELBQADggIBADbp77+S9WYK
K7Pdk8B+Rr06nJiIP2m6RbdY7broY2KgHQaMj5KHamdZgFefrXAnizI6CoURUP0+
lgSxec8FdVdn3R+pj43Anrv0Z/mqsdQdC9+LKJ5qI2P8chpQiI1mJQVdqTfXj/8L
qIxEqqAOVRst+9AhUBtrQOtRsjdYIqInKu1iI57FNOfKnuBgX/iWGhT7P70WiA5i
OdFP7YhJTuyVtYFuyrsHUirqjsMuvEZA6WLuakInweOxRftGrDcJWxhtNI8YpVV+
Zi2R1OlNUWr59SILstiSuFSs2m31cehCsoFT0CE13tniQidmcq3dwxxA7CF4PbDY
bpuLssLFG5xFnyBvve4jd/so5lyT+2i62GrTnaVW/nvgKvqyDtbP7MGH8Nd38d7f
DuPIWcS8zAIm0Catc2K8Y0uOUJLdBbnLx+rtQ/C55I+1Zk9e2XSOIEa1xvUQJFCs
bUUw5zarmZUUYQwIeKBok1Fdtz8YoGY4eCMJQSyZLZd03Ol84EPQgQNiognSAXEv
k18kI6FkUswGiy8tvAXTER/bQw4g/JbPPkMbrBkJR+kwDUEBCNSRZix0OP1dwdeu
+Kqii3kGCDgszvAHoeA0peWCm5KwX3KUHmJ6GtQ1QeLvXaeXnQd+6gB4iuuA50RI
SObEwf+LW1ftJ1igUGT9KF6kuhVJ+Q/w
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
            if url.scheme() == "wss" {
                // Parse the PEM and add to root store
                let mut root_store = RootCertStore::empty();
                 let mut pem = MY_ROOT_CA_PEM.as_bytes();
                let certs: Vec<_> = rustls_pemfile::certs(&mut pem)
                    .collect();
                for cert in certs {
                       let cert = cert.map_err(|_| SignalError::SendError)?;
                     root_store.add(&Certificate(cert.to_vec())).map_err(|_| SignalError::SendError)?;
                    root_store.add(cert).map_err(|_| SignalError::SendError)?;
                }
                let config = ClientConfig::builder()
                    .with_safe_defaults()
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
