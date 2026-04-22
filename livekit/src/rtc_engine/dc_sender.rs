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

/// Bounded, drop-oldest send queue for the [`DataChannelSender`] task.
///
/// When full, the oldest payload is evicted in favour of the newer arrival —
/// preferring freshness over completeness for latency-sensitive data-track
/// publishing. Cloneable so producers and the sender task can share it.
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

    /// Enqueue a payload, returning the evicted oldest payload if the queue
    /// was at capacity.
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

    /// Awaits the next queued payload. Cancel-safe: dropping the future
    /// leaves any queued payload in place.
    async fn recv(&self) -> Bytes {
        loop {
            if let Some(payload) = self.try_pop() {
                return payload;
            }
            // Register for wake-up before rechecking to avoid a missed notify.
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

/// Sender task for the `_data_track` RTC data channel, with
/// buffered-amount backpressure.
///
/// Forwards opaque `Bytes` verbatim (no encoding, sequencing, or retries)
/// and holds only the freshest pending payload via [`DataTrackSendQueue`].
/// Not used for reliable/lossy data packets, which go through
/// `SessionInner::data_channel_task` since they need unbounded queueing,
/// retries, and per-kind sequencing.
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
    /// Queue capacity. Set to 1 so only the freshest pending payload is
    /// held; any older queued payload is evicted on the next send.
    const QUEUE_CAPACITY: usize = 1;

    /// Creates a new sender.
    ///
    /// Returns a tuple containing the following:
    /// - The sender itself to be spawned by the caller (see [`DataChannelSender::run`]).
    /// - A cloneable queue handle for producers to push payloads onto.
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
