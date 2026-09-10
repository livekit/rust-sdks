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
use prost::Message as ProtoMessage;
use std::{sync::Arc, time::Duration};
use tokio::task::JoinHandle;

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

/// Grace period for the read and write tasks to stop during [`SignalStream::close`].
///
/// A link that dies without FIN or RST (a lost cellular or Wi-Fi uplink) leaves the
/// transport parked with nothing to wake it: `recv` never returns and `send` waits on
/// TCP retransmission. `close` runs on the room teardown and reconnect paths, so it has
/// to stay bounded whatever the peer does.
const CLOSE_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// SignalStream holds the WebSocket connection (via `WsConnection`).
///
/// It is replaced by [SignalClient] at each reconnection.
#[derive(Debug)]
pub(super) struct SignalStream {
    internal_tx: mpsc::Sender<InternalMessage>,
    /// Dropped to stop `read_task`, which is otherwise parked in a `recv()` that a dead
    /// link never completes. Held by value, so dropping the `SignalStream` without
    /// calling [`SignalStream::close`] stops the reader too.
    shutdown_tx: oneshot::Sender<()>,
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
        log::info!("connecting to {}", livekit_net::redact_url(&url));

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
        let conn = tokio::time::timeout(
            connect_timeout,
            transport.connect(url.to_string(), headers, connect_timeout.as_millis() as u64),
        )
        .await
        .map_err(|_| SignalError::Timeout("signal connection timed out".into()))??
        .connection;

        let (emitter, events) = mpsc::unbounded_channel();
        let (internal_tx, internal_rx) = mpsc::channel::<InternalMessage>(8);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let write_handle = tokio::spawn(Self::write_task(internal_rx, conn.clone()));
        let read_handle =
            tokio::spawn(Self::read_task(internal_tx.clone(), conn, emitter, shutdown_rx));

        Ok((Self { internal_tx, shutdown_tx, read_handle, write_handle }, events))
    }

    /// Close the websocket.
    /// It sends a Close message before closing.
    ///
    /// Bounded by [`CLOSE_DRAIN_TIMEOUT`]: callers hold the stream lock across this call
    /// and the room teardown goes through it, so it must not depend on the peer. A link
    /// that dies without FIN or RST never delivers the close the read task waits for, so
    /// stop that task instead of waiting for the transport.
    pub async fn close(self, notify_close: bool) {
        let Self { internal_tx, shutdown_tx, mut read_handle, mut write_handle } = self;

        if notify_close {
            // Best effort: the peer may already be unreachable, and the channel has a
            // capacity of 8 that a stalled write task can fill.
            let _ =
                tokio::time::timeout(CLOSE_DRAIN_TIMEOUT, internal_tx.send(InternalMessage::Close))
                    .await;
        }

        // Stop the read task first: it holds a clone of `internal_tx`, so the write
        // task's channel only closes once the read task is gone.
        drop(shutdown_tx);
        drop(internal_tx);

        let drained = tokio::time::timeout(CLOSE_DRAIN_TIMEOUT, async {
            let _ = (&mut read_handle).await;
            let _ = (&mut write_handle).await;
        })
        .await;

        if drained.is_err() {
            // A transport that stops responding some other way (a `send` waiting on TCP
            // retransmission, say) would otherwise keep the task, and with it the last
            // references to the connection, alive after the caller has given up. Stop
            // them so the socket is dropped rather than leaked.
            log::warn!("signal stream did not shut down within {:?}", CLOSE_DRAIN_TIMEOUT);
            read_handle.abort();
            write_handle.abort();
        }
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
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        loop {
            // `recv` never returns on a link that delivers no data: no data, no FIN, no
            // RST. Without this arm the task outlives its `SignalStream` and blocks
            // every caller that awaits its `JoinHandle`.
            let received = tokio::select! {
                result = conn.recv() => result,
                _ = &mut shutdown_rx => break,
            };
            match received {
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

#[cfg(test)]
mod tests {
    use super::*;
    use livekit_net::TransportError;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// A transport that delivers no data and never closes the read side, like a cellular
    /// or Wi-Fi uplink that goes away without FIN or RST. `close` only records the call,
    /// since `NativeConnection::close` closes the writer alone.
    struct StalledConn {
        closed: AtomicBool,
    }

    #[async_trait::async_trait]
    impl livekit_net::WsConnection for StalledConn {
        async fn send(&self, _frame: Vec<u8>) -> Result<(), TransportError> {
            Ok(())
        }

        async fn recv(&self) -> Result<Option<Vec<u8>>, TransportError> {
            std::future::pending().await
        }

        async fn close(&self) {
            self.closed.store(true, Ordering::SeqCst);
        }
    }

    /// A `SignalStream` wired up exactly as `connect` does, over a stalled transport.
    fn stalled_stream() -> (SignalStream, Arc<StalledConn>) {
        let conn = Arc::new(StalledConn { closed: AtomicBool::new(false) });
        let dyn_conn: Arc<dyn livekit_net::WsConnection> = conn.clone();
        let (emitter, _events) = mpsc::unbounded_channel();
        let (internal_tx, internal_rx) = mpsc::channel::<InternalMessage>(8);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let write_handle = tokio::spawn(SignalStream::write_task(internal_rx, dyn_conn.clone()));
        let read_handle = tokio::spawn(SignalStream::read_task(
            internal_tx.clone(),
            dyn_conn,
            emitter,
            shutdown_rx,
        ));
        (SignalStream { internal_tx, shutdown_tx, read_handle, write_handle }, conn)
    }

    /// Wait for the transport to be closed, or give up. The tasks run on other threads,
    /// so the flag is not set the instant `close` returns.
    async fn wait_for_close(conn: &Arc<StalledConn>) -> bool {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !conn.closed.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok()
    }

    /// The room teardown and the reconnect both go through `close`. A link that stops
    /// delivering data leaves `recv` parked forever, so without a way to stop the read
    /// task this join never returns and the caller can neither leave the room nor
    /// reconnect.
    #[tokio::test(flavor = "multi_thread")]
    async fn close_returns_on_a_dead_link() {
        let (stream, conn) = stalled_stream();

        let result = tokio::time::timeout(Duration::from_secs(10), stream.close(true)).await;

        assert!(result.is_ok(), "SignalStream::close() did not return on a dead link");
        assert!(wait_for_close(&conn).await, "conn.close() was never called");
    }

    /// `restart` closes the old stream without notifying the peer. That path has no
    /// `Close` message to unblock the write task, so it relies on the same shutdown.
    #[tokio::test(flavor = "multi_thread")]
    async fn close_without_notify_returns_on_a_dead_link() {
        let (stream, conn) = stalled_stream();

        let result = tokio::time::timeout(Duration::from_secs(10), stream.close(false)).await;

        assert!(result.is_ok(), "SignalStream::close(false) did not return on a dead link");
        assert!(wait_for_close(&conn).await, "conn.close() was never called");
    }

    /// A caller that abandons `close` drops the stream instead. The shutdown sender is
    /// owned by the stream, so that drop has to stop the tasks as well — otherwise they
    /// stay parked on the socket for the life of the process.
    #[tokio::test(flavor = "multi_thread")]
    async fn dropping_the_stream_stops_its_tasks() {
        let (stream, conn) = stalled_stream();

        drop(stream);

        assert!(wait_for_close(&conn).await, "a dropped stream left its tasks running");
    }
}
