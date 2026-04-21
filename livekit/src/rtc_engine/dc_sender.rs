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

use bytes::Bytes;
use libwebrtc::{self as rtc, data_channel::DataChannel};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch, Notify};

/// Options for constructing a [`DataChannelSender`].
pub struct DataChannelSenderOptions {
    pub low_buffer_threshold: u64,
    pub dc: DataChannel,
    pub close_rx: watch::Receiver<bool>,
}

/// Bounded, drop-oldest send queue handed to the [`DataChannelSender`] task.
///
/// When the queue is at capacity, [`DataTrackSendQueue::send`] evicts the
/// *oldest* payload to make room for the newer arrival. This policy favours
/// freshness over completeness, which is the desired behaviour for
/// latency-sensitive data-track publishing: a stale sample queued behind a
/// congested DC is always less useful than the freshest one, so we'd rather
/// drop the stale one and get the latest datum on the wire as soon as
/// possible.
///
/// The queue is cloneable: producers hold one clone, the sender task holds
/// another.
#[derive(Clone)]
pub struct DataTrackSendQueue {
    inner: Arc<DataTrackSendQueueInner>,
}

struct DataTrackSendQueueInner {
    queue: Mutex<VecDeque<Bytes>>,
    notify: Notify,
    capacity: usize,
}

impl DataTrackSendQueue {
    fn new(capacity: usize) -> Self {
        debug_assert!(capacity >= 1);
        Self {
            inner: Arc::new(DataTrackSendQueueInner {
                queue: Mutex::new(VecDeque::with_capacity(capacity)),
                notify: Notify::new(),
                capacity,
            }),
        }
    }

    /// Enqueue a payload. Always accepts the new payload; if the queue was
    /// already at capacity, the oldest payload is evicted and returned so
    /// callers can observe drops (e.g. for logging/metrics).
    pub fn send(&self, payload: Bytes) -> Option<Bytes> {
        let mut queue = self.inner.queue.lock().expect("send queue mutex poisoned");
        let dropped = if queue.len() >= self.inner.capacity { queue.pop_front() } else { None };
        queue.push_back(payload);
        drop(queue);
        self.inner.notify.notify_one();
        dropped
    }

    fn try_pop(&self) -> Option<Bytes> {
        self.inner.queue.lock().expect("send queue mutex poisoned").pop_front()
    }

    fn drain(&self) -> VecDeque<Bytes> {
        std::mem::take(&mut *self.inner.queue.lock().expect("send queue mutex poisoned"))
    }

    /// Awaits and returns the next queued payload.
    ///
    /// The future is cancel-safe: dropping it without completion leaves any
    /// still-queued payload in place for the next call.
    async fn recv(&self) -> Bytes {
        loop {
            if let Some(payload) = self.try_pop() {
                return payload;
            }
            // Register interest *before* rechecking the queue to avoid a
            // missed wake if `send` runs between our pop and our await.
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(payload) = self.try_pop() {
                return payload;
            }
            notified.await;
        }
    }
}

/// A task responsible for sending data-track payloads over a single RTC data
/// channel.
///
/// This implements buffered-amount-based backpressure similar to
/// `SessionInner::data_channel_task`, but is specialised for the data-track
/// DC (`_data_track`):
///
/// - The producer-facing queue ([`DataTrackSendQueue`]) is bounded to
///   [`Self::QUEUE_CAPACITY`] with drop-oldest semantics, so only the freshest
///   pending payload is ever held. This is appropriate for latency-sensitive
///   data tracks, where a stale sample queued behind a congested DC is worse
///   than simply dropping it in favour of a newer one.
/// - There is no encoding, sequencing, or retry layer; the sender forwards
///   opaque `Bytes` payloads verbatim.
///
/// It is intentionally *not* used for reliable/lossy data-packet publishing
/// (chat, transcription, DTMF, RPC, user packets, streams, etc.) — that path
/// has different requirements (unbounded queueing, retries, per-kind
/// sequencing) and continues to live in `SessionInner::data_channel_task`.
///
/// In a future refactor, it would be worth revisiting how the logic in
/// `SessionInner::data_channel_task` can be decoupled from session likely by
/// reusing a generalised version of this sender and moving encoding and retry
/// concerns into a separate layer (see the `livekit-datatrack` crate for an
/// example of this approach). Doing that would require making the queue
/// capacity / eviction policy configurable rather than hardcoded to
/// drop-oldest, capacity 1.
///
pub struct DataChannelSender {
    /// Drop-oldest queue of payloads waiting for the DC to drain.
    queue: DataTrackSendQueue,

    /// Channel for receiving events from the data channel.
    dc_event_rx: mpsc::UnboundedReceiver<DataChannelEvent>,

    dc_event_tx: mpsc::UnboundedSender<DataChannelEvent>,

    /// Receiver used to end the task when changed.
    close_rx: watch::Receiver<bool>,

    /// Reference to the data channel to use for sending.
    dc: DataChannel,

    low_buffer_threshold: u64,

    /// Number of bytes in the data channel's internal buffer.
    buffered_amount: u64,
}

impl DataChannelSender {
    /// Maximum number of payloads held in [`DataTrackSendQueue`] waiting to be
    /// handed to the DC.
    ///
    /// Kept at 1 so we only ever hold the single freshest pending payload.
    /// When a newer payload arrives while the DC is still draining an older
    /// queued one, the older one is evicted.
    const QUEUE_CAPACITY: usize = 1;

    /// Creates a new sender.
    ///
    /// Returns a tuple containing the following:
    /// - The sender itself to be spawned by the caller (see [`DataChannelSender::run`]).
    /// - A cloneable queue handle for producers to push payloads onto.
    ///
    pub fn new(options: DataChannelSenderOptions) -> (Self, DataTrackSendQueue) {
        let queue = DataTrackSendQueue::new(Self::QUEUE_CAPACITY);
        let (dc_event_tx, dc_event_rx) = mpsc::unbounded_channel();

        let sender = Self {
            low_buffer_threshold: options.low_buffer_threshold,
            dc: options.dc,
            queue: queue.clone(),
            dc_event_rx,
            dc_event_tx,
            close_rx: options.close_rx,
            buffered_amount: 0,
        };
        (sender, queue)
    }

    /// Run the sender task, consuming self.
    ///
    /// The sender will continue running until `close_rx` changes.
    ///
    pub async fn run(mut self) {
        log::debug!("Send task started for data channel '{}'", self.dc.label());
        self.register_dc_callbacks();
        loop {
            tokio::select! {
                Some(event) = self.dc_event_rx.recv() => {
                    let DataChannelEvent::BytesSent(bytes_sent) = event;
                    self.handle_bytes_sent(bytes_sent);
                }
                payload = self.queue.recv(),
                    if self.buffered_amount <= self.low_buffer_threshold =>
                {
                    self.dispatch(payload);
                }
                _ = self.close_rx.changed() => break
            }
        }

        let remaining = self.queue.drain();
        if !remaining.is_empty() {
            let unsent_bytes: usize = remaining.iter().map(|p| p.len()).sum();
            log::info!("{} byte(s) remain in queue", unsent_bytes);
        }
        log::debug!("Send task ended for data channel '{}'", self.dc.label());
    }

    fn dispatch(&mut self, payload: Bytes) {
        self.buffered_amount += payload.len() as u64;
        _ = self
            .dc
            .send(&payload, true)
            .inspect_err(|err| log::error!("Failed to send data: {}", err));
    }

    fn handle_bytes_sent(&mut self, bytes_sent: u64) {
        if self.buffered_amount < bytes_sent {
            log::error!("Unexpected buffer amount");
            self.buffered_amount = 0;
            return;
        }
        self.buffered_amount -= bytes_sent;
    }

    fn register_dc_callbacks(&self) {
        self.dc.on_buffered_amount_change(
            on_buffered_amount_change(self.dc_event_tx.downgrade()).into(),
        );
    }
}

/// Event produced by the RTC data channel.
#[derive(Debug)]
enum DataChannelEvent {
    /// Indicates the specified number of bytes have been sent.
    BytesSent(u64),
}
// Note: if we also need to know when the data channel's state changes,
// we can add an event for that here and register another callback following this same pattern.

fn on_buffered_amount_change(
    event_tx: mpsc::WeakUnboundedSender<DataChannelEvent>,
) -> rtc::data_channel::OnBufferedAmountChange {
    Box::new(move |bytes_sent| {
        // Note: this callback is holding onto a weak sender, which will not
        // prevent the channel from closing.
        let Some(event_tx) = event_tx.upgrade() else { return };
        _ = event_tx.send(DataChannelEvent::BytesSent(bytes_sent));
    })
}
