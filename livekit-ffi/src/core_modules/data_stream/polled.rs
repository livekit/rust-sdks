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

//! Pull-based adapters over the push delegates, for bindings whose callbacks are thread-affine.
//!
//! [`incoming`](super::incoming) and [`outgoing`](super::outgoing) surface their output by calling
//! a foreign delegate from this crate's tokio runtime. Some bindings cannot accept that. Dart is
//! the motivating case: uniffi compiles a callback interface to `Pointer.fromFunction`, which is
//! only valid on the thread owning the isolate, so a delegate invoked from a tokio worker aborts
//! the VM outright with "Cannot invoke native callback outside an isolate" — not a catchable
//! error. (Dart's thread-safe callback form, `NativeCallable.listener`, is asynchronous and cannot
//! return a value, so it can't satisfy uniffi's synchronous callback ABI either.)
//!
//! The fix is to keep the delegate on the Rust side. Each type here implements the relevant
//! delegate trait, buffers what it receives into a channel, and exposes an `async fn next_*` the
//! foreign side awaits. Nothing crosses the FFI until that await resolves, and uniffi polls those
//! futures from whichever thread called `rust_future_poll` — the binding's own. Delegate
//! invocation still happens on a tokio thread, which is fine precisely because the implementation
//! is Rust.
//!
//! The outgoing side also keeps the delegate's acknowledgement contract. Each delegate call becomes
//! an [`OutgoingPacketBatch`] that the foreign side acknowledges once its packets are handed to
//! the transport, so a send still waits for the transport and still fails when the transport does.
//!
//! Note that [`RemoteParticipantRegistryDelegate`] needs no adapter: it is only ever called
//! synchronously inside a `send_*` future, so it already runs on the polling thread.
//!
//! Bindings that can take callbacks from any thread (Swift, Kotlin) should ignore this module and
//! construct the managers directly.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::{oneshot, Mutex};
use tokio_util::sync::CancellationToken;

use super::common::PacketDeliveryError;
use super::incoming::{
    ByteStreamReader, IncomingDataStreamManager, IncomingDataStreamManagerDelegate,
    TextStreamReader,
};
use super::outgoing::{
    OutgoingDataStreamManager, OutgoingDataStreamManagerDelegate, RemoteParticipantRegistryDelegate,
};

/// Queue depth at which we start warning. The channels are unbounded so a stalled consumer can't
/// deadlock the manager, which means the only backstop against unbounded growth is noticing.
const QUEUE_DEPTH_WARN: usize = 1024;

fn warn_if_deep(kind: &str, depth: usize) {
    if depth == QUEUE_DEPTH_WARN {
        log::warn!(
            "{kind} queue has reached {depth} pending items; the foreign side is not draining it \
             fast enough"
        );
    }
}

// MARK: - Outgoing

/// One delegate call's worth of outbound packets, pulled by the foreign side.
///
/// The `send_*`/`write` call that produced it stays pending until the foreign side reports on
/// [`Self::ack`], which keeps the delegate contract: a send resolves only once its packets are
/// handed to the transport, and a transport failure fails it.
#[derive(uniffi::Record)]
pub struct OutgoingPacketBatch {
    /// Encoded `livekit.DataPacket`s to hand to the transport, in order.
    pub packets: Vec<Bytes>,
    /// Where to report what happened to them.
    pub ack: Arc<OutgoingBatchAck>,
}

type OutcomeSender = oneshot::Sender<Result<(), PacketDeliveryError>>;

/// Reports what happened to the packets of an [`OutgoingPacketBatch`].
///
/// Released without an outcome it fails the send, so dropping a batch can never acknowledge packets
/// that were not sent.
#[derive(uniffi::Object)]
pub struct OutgoingBatchAck {
    sender: std::sync::Mutex<Option<OutcomeSender>>,
}

impl OutgoingBatchAck {
    fn resolve(&self, result: Result<(), PacketDeliveryError>) {
        let sender = self.sender.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
        if let Some(sender) = sender {
            // The send may already be gone (its manager shut down), in which case nobody is waiting.
            let _ = sender.send(result);
        }
    }
}

impl Drop for OutgoingBatchAck {
    fn drop(&mut self) {
        let sender = self.sender.get_mut().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
        if let Some(sender) = sender {
            let _ = sender.send(Err(PacketDeliveryError::Failed {
                reason: "packet batch released without an outcome".to_string(),
            }));
        }
    }
}

#[uniffi::export]
impl OutgoingBatchAck {
    /// Reports every packet as handed to the transport, resolving the originating send.
    ///
    /// Only the first of [`Self::complete`] and [`Self::fail`] counts.
    pub fn complete(&self) {
        self.resolve(Ok(()));
    }

    /// Reports that the packets could not be delivered, failing the originating send.
    ///
    /// Only the first of [`Self::complete`] and [`Self::fail`] counts.
    pub fn fail(&self, reason: String) {
        self.resolve(Err(PacketDeliveryError::Failed { reason }));
    }
}

/// A batch on its way from the sink to the queue, before the foreign side has pulled it.
struct PendingBatch {
    packets: Vec<Bytes>,
    outcome: OutcomeSender,
}

/// Hands outbound packets to the foreign side one batch at a time.
///
/// This is the receiving half, held by the foreign side. The manager holds the sending half,
/// [`OutgoingPacketSink`]. The manager awaits each delegate call before making the next, and each
/// call waits for its batch's outcome, so at most one batch is pending at a time.
#[derive(uniffi::Object)]
pub struct OutgoingPacketQueue {
    rx: Mutex<UnboundedReceiver<PendingBatch>>,
    shutdown: CancellationToken,
}

/// The sending half of an [`OutgoingPacketQueue`], which implements
/// [`OutgoingDataStreamManagerDelegate`] in Rust; see the module docs.
///
/// The halves are separate objects so that the manager's forwarding task, which holds this one,
/// does not keep the receiving half alive. Releasing the queue then drops any batch nobody pulled,
/// which fails its send and lets the task finish. The task also finishes when the manager is
/// dropped, whatever the foreign side still holds.
pub(super) struct OutgoingPacketSink {
    tx: UnboundedSender<PendingBatch>,
    shutdown: CancellationToken,
    manager_shutdown: CancellationToken,
}

impl OutgoingPacketQueue {
    /// `manager_shutdown` is the token the manager cancels when it is dropped.
    pub(super) fn pair(
        manager_shutdown: CancellationToken,
    ) -> (Arc<OutgoingPacketSink>, Arc<Self>) {
        let (tx, rx) = unbounded_channel();
        let shutdown = CancellationToken::new();
        let sink = OutgoingPacketSink { tx, shutdown: shutdown.clone(), manager_shutdown };
        (Arc::new(sink), Arc::new(Self { rx: Mutex::new(rx), shutdown }))
    }
}

fn queue_closed() -> PacketDeliveryError {
    PacketDeliveryError::Failed { reason: "outgoing packet queue closed".to_string() }
}

fn manager_closed() -> PacketDeliveryError {
    PacketDeliveryError::Failed { reason: "outgoing data stream manager closed".to_string() }
}

#[async_trait::async_trait]
impl OutgoingDataStreamManagerDelegate for OutgoingPacketSink {
    async fn on_packets_available(&self, packets: Vec<Bytes>) -> Result<(), PacketDeliveryError> {
        let (outcome_tx, outcome_rx) = oneshot::channel();
        self.tx.send(PendingBatch { packets, outcome: outcome_tx }).map_err(|_| queue_closed())?;
        tokio::select! {
            _ = self.shutdown.cancelled() => Err(queue_closed()),
            _ = self.manager_shutdown.cancelled() => Err(manager_closed()),
            // The sender is dropped unused only when the queue goes away with the batch unpulled.
            outcome = outcome_rx => outcome.unwrap_or_else(|_| Err(queue_closed())),
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl OutgoingPacketQueue {
    /// Awaits the next batch of packets to hand to the transport.
    ///
    /// Report on the batch's `ack` once its packets are sent (or have failed); the next batch
    /// is not produced until then. Returns `None` once the queue is closed, which ends
    /// the caller's drain loop.
    pub async fn next_batch(&self) -> Option<OutgoingPacketBatch> {
        let mut rx = self.rx.lock().await;
        let pending = tokio::select! {
            _ = self.shutdown.cancelled() => return None,
            received = rx.recv() => received?,
        };
        let sender = std::sync::Mutex::new(Some(pending.outcome));
        Some(OutgoingPacketBatch {
            packets: pending.packets,
            ack: Arc::new(OutgoingBatchAck { sender }),
        })
    }

    /// Wakes a pending [`Self::next_batch`] with `None` so the caller's drain loop can exit, and
    /// fails any send still waiting on a batch.
    ///
    /// Call this before releasing the queue: a caller blocked in `next_batch` is holding a pointer
    /// to it, so freeing it first is a use-after-free.
    pub fn close(&self) {
        self.shutdown.cancel();
    }
}

/// An [`OutgoingDataStreamManager`] and the queue draining it, already connected.
#[derive(uniffi::Record)]
pub struct PolledOutgoingDataStreamManager {
    pub manager: Arc<OutgoingDataStreamManager>,
    pub packets: Arc<OutgoingPacketQueue>,
}

/// Builds an outgoing manager whose packets are pulled rather than pushed.
///
/// The two halves are wired together here rather than by the caller: passing a Rust object where
/// `Arc<dyn OutgoingDataStreamManagerDelegate>` is expected is awkward-to-impossible from some
/// bindings, and unnecessary — it's ordinary Rust on this side.
#[uniffi::export]
pub fn polled_outgoing_data_stream_manager(
    registry: Arc<dyn RemoteParticipantRegistryDelegate>,
) -> PolledOutgoingDataStreamManager {
    let manager_shutdown = CancellationToken::new();
    let (sink, packets) = OutgoingPacketQueue::pair(manager_shutdown.clone());
    let manager = OutgoingDataStreamManager::with_shutdown(sink, registry, manager_shutdown);
    PolledOutgoingDataStreamManager { manager, packets }
}

// MARK: - Incoming

/// A stream opened by a remote participant.
///
/// Exactly one of the two readers is set; which one tells you the stream's kind. Two `Option`s
/// rather than an enum keeps the shape trivial in every binding.
#[derive(uniffi::Record)]
pub struct OpenedStream {
    /// Identity of the participant that opened the stream.
    pub identity: String,
    /// Set when the stream carries bytes.
    pub byte_reader: Option<Arc<ByteStreamReader>>,
    /// Set when the stream carries text.
    pub text_reader: Option<Arc<TextStreamReader>>,
}

/// A stream closed by a remote participant (or terminated by an error/abort); see
/// [`IncomingDataStreamManagerDelegate::on_stream_closed`].
#[derive(uniffi::Record)]
pub struct ClosedStream {
    /// Id of the stream that closed.
    pub stream_id: String,
    /// Identity of the participant that opened the stream.
    pub identity: String,
}

/// Buffers opened and closed streams so they can be pulled instead of pushed.
///
/// Implements [`IncomingDataStreamManagerDelegate`] in Rust; see the module docs.
#[derive(uniffi::Object)]
pub struct IncomingStreamQueue {
    tx: UnboundedSender<OpenedStream>,
    rx: Mutex<UnboundedReceiver<OpenedStream>>,
    depth: AtomicUsize,
    closed_tx: UnboundedSender<ClosedStream>,
    closed_rx: Mutex<UnboundedReceiver<ClosedStream>>,
    closed_depth: AtomicUsize,
    shutdown: CancellationToken,
}

impl IncomingStreamQueue {
    fn new() -> Self {
        let (tx, rx) = unbounded_channel();
        let (closed_tx, closed_rx) = unbounded_channel();
        Self {
            tx,
            rx: Mutex::new(rx),
            depth: AtomicUsize::new(0),
            closed_tx,
            closed_rx: Mutex::new(closed_rx),
            closed_depth: AtomicUsize::new(0),
            shutdown: CancellationToken::new(),
        }
    }

    fn push(&self, opened: OpenedStream) {
        if self.tx.send(opened).is_ok() {
            warn_if_deep("incoming stream", self.depth.fetch_add(1, Ordering::Relaxed) + 1);
        }
    }
}

impl IncomingDataStreamManagerDelegate for IncomingStreamQueue {
    fn on_byte_stream_opened(&self, reader: Arc<ByteStreamReader>, identity: String) {
        self.push(OpenedStream { identity, byte_reader: Some(reader), text_reader: None });
    }

    fn on_text_stream_opened(&self, reader: Arc<TextStreamReader>, identity: String) {
        self.push(OpenedStream { identity, byte_reader: None, text_reader: Some(reader) });
    }

    fn on_stream_closed(&self, stream_id: String, identity: String) {
        if self.closed_tx.send(ClosedStream { stream_id, identity }).is_ok() {
            warn_if_deep("closed stream", self.closed_depth.fetch_add(1, Ordering::Relaxed) + 1);
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl IncomingStreamQueue {
    /// Awaits the next stream opened by a remote participant.
    ///
    /// Returns `None` once the manager has shut down, which ends the caller's drain loop. Unlike
    /// [`OutgoingPacketQueue::next_batch`] this yields one at a time: each carries a reader the
    /// caller has to route to a handler, so batching would only defer that work.
    pub async fn next_opened_stream(&self) -> Option<OpenedStream> {
        let mut rx = self.rx.lock().await;
        let opened = tokio::select! {
            _ = self.shutdown.cancelled() => return None,
            received = rx.recv() => received?,
        };
        self.depth.fetch_sub(1, Ordering::Relaxed);
        Some(opened)
    }

    /// Awaits the next stream-closed notification.
    ///
    /// Pulled independently of [`Self::next_opened_stream`], so ordering across the two queues is
    /// not guaranteed — correlate by `stream_id` (a close always follows its open on the push
    /// side). Returns `None` once the manager has shut down.
    pub async fn next_closed_stream(&self) -> Option<ClosedStream> {
        let mut rx = self.closed_rx.lock().await;
        let closed = tokio::select! {
            _ = self.shutdown.cancelled() => return None,
            received = rx.recv() => received?,
        };
        self.closed_depth.fetch_sub(1, Ordering::Relaxed);
        Some(closed)
    }

    /// Wakes a pending [`Self::next_opened_stream`] or [`Self::next_closed_stream`] with `None`.
    /// See [`OutgoingPacketQueue::close`].
    pub fn close(&self) {
        self.shutdown.cancel();
    }
}

/// An [`IncomingDataStreamManager`] and the queue draining it, already connected.
#[derive(uniffi::Record)]
pub struct PolledIncomingDataStreamManager {
    pub manager: Arc<IncomingDataStreamManager>,
    pub streams: Arc<IncomingStreamQueue>,
}

/// Builds an incoming manager whose opened streams are pulled rather than pushed.
///
/// `max_payload_byte_length` is fixed for the lifetime of the manager; see
/// [`IncomingDataStreamManager::new`]. Hosts sourcing it from per-connection options should build
/// a fresh manager per session rather than memoizing one.
#[uniffi::export]
pub fn polled_incoming_data_stream_manager(
    max_payload_byte_length: Option<u64>,
) -> PolledIncomingDataStreamManager {
    let streams = Arc::new(IncomingStreamQueue::new());
    let manager = IncomingDataStreamManager::new(streams.clone(), max_payload_byte_length);
    PolledIncomingDataStreamManager { manager, streams }
}
