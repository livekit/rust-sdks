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

use livekit_protocol as proto;
use livekit_runtime::JoinHandle;
use prost::Message as ProtoMessage;
use std::{sync::Arc, time::Duration};

use tokio::sync::{mpsc, oneshot};

use super::{SignalError, SignalResult};

#[derive(Debug)]
enum InternalMessage {
    Signal {
        signal: proto::signal_request::Message,
        response_chn: oneshot::Sender<SignalResult<()>>,
    },
    Close,
}

/// SignalStream holds the WebSocket connection (via `WsConnection`).
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
    /// Returns SignalError if the connection failed.
    ///
    /// SignalStream will never try to reconnect if the connection has been closed.
    pub async fn connect(
        url: url::Url,
        token: &str,
        connect_timeout: Duration,
    ) -> SignalResult<(Self, mpsc::UnboundedReceiver<Box<proto::signal_response::Message>>)> {
        log::info!("connecting to {}", url);

        // Reject a malformed token before touching the transport, so it surfaces
        // as a non-retryable TokenFormat error rather than a generic connection
        // failure that the caller would pointlessly retry.
        super::check_token_format(token)?;

        let transport = super::require_ws_client()?;

        let headers = super::bearer_headers(token);

        // Delegate the connect deadline to the transport via `timeout_ms`, but keep
        // an outer Rust-side timeout as a backstop: a foreign/host transport that
        // ignores or mishandles `timeout_ms` must not be able to hang connect (and
        // thus the engine's reconnect loop) forever.
        let conn = livekit_runtime::timeout(
            connect_timeout,
            transport.connect(url.to_string(), headers, connect_timeout.as_millis() as u64),
        )
        .await
        .map_err(|_| SignalError::Timeout("signal connection timed out".into()))??
        .connection;

        let (emitter, events) = mpsc::unbounded_channel();
        let (internal_tx, internal_rx) = mpsc::channel::<InternalMessage>(8);
        let write_handle = livekit_runtime::spawn(Self::write_task(internal_rx, conn.clone()));
        let read_handle =
            livekit_runtime::spawn(Self::read_task(internal_tx.clone(), conn, emitter));

        Ok((Self { internal_tx, read_handle, write_handle }, events))
    }

    /// Close the websocket.
    /// It sends a Close message before closing.
    pub async fn close(self, notify_close: bool) {
        if notify_close {
            let _ = self.internal_tx.send(InternalMessage::Close).await;
        }
        let _ = self.write_handle.await;
        let _ = self.read_handle.await;
    }

    /// Send a SignalRequest to the websocket.
    /// It also waits for the message to be sent.
    pub async fn send(&self, signal: proto::signal_request::Message) -> SignalResult<()> {
        let (send, recv) = oneshot::channel();
        let msg = InternalMessage::Signal { signal, response_chn: send };
        let _ = self.internal_tx.send(msg).await;
        recv.await.map_err(|_| SignalError::SendError)?
    }

    /// This task is used to send messages to the websocket.
    /// It is also responsible for closing the connection.
    async fn write_task(
        mut internal_rx: mpsc::Receiver<InternalMessage>,
        conn: Arc<dyn livekit_net::WsConnection>,
    ) {
        while let Some(msg) = internal_rx.recv().await {
            match msg {
                InternalMessage::Signal { signal, response_chn } => {
                    let data = proto::SignalRequest { message: Some(signal) }.encode_to_vec();

                    if let Err(err) = conn.send(data).await {
                        // A send failure is a broken/closed socket, not a timeout —
                        // map it through the shared taxonomy so callers branching on
                        // Connection vs Timeout take the right path.
                        let _ = response_chn.send(Err(err.into()));
                        break;
                    }

                    let _ = response_chn.send(Ok(()));
                }
                InternalMessage::Close => break,
            }
        }

        conn.close().await;
    }

    /// This task is used to read incoming messages from the websocket
    /// and dispatch them through the EventEmitter.
    async fn read_task(
        internal_tx: mpsc::Sender<InternalMessage>,
        conn: Arc<dyn livekit_net::WsConnection>,
        emitter: mpsc::UnboundedSender<Box<proto::signal_response::Message>>,
    ) {
        loop {
            match conn.recv().await {
                Ok(Some(bytes)) => {
                    match proto::SignalResponse::decode(bytes.as_slice()) {
                        Ok(res) => {
                            if let Some(msg) = res.message {
                                let _ = emitter.send(Box::new(msg));
                            }
                        }
                        Err(e) => {
                            log::error!("failed to decode SignalResponse: {:?}", e);
                            // continue on decode error — don't tear down the connection
                        }
                    }
                }
                Ok(None) => {
                    // Peer/transport closed gracefully
                    let _ = internal_tx.send(InternalMessage::Close).await;
                    break;
                }
                Err(e) => {
                    log::error!("websocket recv error: {:?}", e);
                    let _ = internal_tx.send(InternalMessage::Close).await;
                    break;
                }
            }
        }
    }
}
