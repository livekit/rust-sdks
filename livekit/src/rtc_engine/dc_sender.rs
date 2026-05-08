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

/// A single application-level frame's worth of serialized packets.
///
/// One frame may serialize into multiple MTU-sized packets; the receiver needs
/// all of them to reassemble the frame. We therefore keep the group together
/// as an atomic unit all the way down to the DC send path so eviction never
/// leaves partially queued frame.
pub type DataTrackFramePackets = Vec<Bytes>;

/// Options for constructing a [`DataChannelSender`].
pub struct DataChannelSenderOptions {
    pub low_buffer_threshold: u64,
    pub dc: DataChannel,
    pub close_rx: watch::Receiver<bool>,
}

/// Bounded, drop-oldest send queue for the [`DataChannelSender`] task.
///
/// Each queue slot holds a [`DataTrackFramePackets`] — the full set of packets
/// for one application frame. When full, the oldest *frame* is evicted in
/// favour of the newer arrival; partial frames are never queued, which keeps
/// reassembly on the receiver side intact.
///
/// Cloneable so producers and the sender task can share it.
#[derive(Clone)]
pub struct DataTrackSendQueue {
    inner: Arc<DataTrackSendQueueInner>,
}

struct DataTrackSendQueueInner {
    queue: Mutex<VecDeque<DataTrackFramePackets>>,
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

    /// Enqueue all packets for a single frame, returning the evicted oldest
    /// frame if the queue was at capacity.
    pub fn send(&self, packets: DataTrackFramePackets) -> Option<DataTrackFramePackets> {
        if packets.is_empty() {
            return None;
        }
        let mut queue = self.inner.queue.lock().expect("send queue mutex poisoned");
        let dropped = if queue.len() >= self.inner.capacity { queue.pop_front() } else { None };
        queue.push_back(packets);
        drop(queue);
        self.inner.notify.notify_one();
        dropped
    }

    fn try_pop(&self) -> Option<DataTrackFramePackets> {
        self.inner.queue.lock().expect("send queue mutex poisoned").pop_front()
    }

    fn drain(&self) -> VecDeque<DataTrackFramePackets> {
        std::mem::take(&mut *self.inner.queue.lock().expect("send queue mutex poisoned"))
    }

    /// Awaits the next queued frame. Cancel-safe: dropping the future leaves
    /// any queued frame in place.
    async fn recv(&self) -> DataTrackFramePackets {
        loop {
            if let Some(packets) = self.try_pop() {
                return packets;
            }
            // Register for wake-up before rechecking to avoid a missed notify.
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(packets) = self.try_pop() {
                return packets;
            }
            notified.await;
        }
    }
}

/// Sender task for the `_data_track` RTC data channel, with
/// buffered-amount backpressure.
///
/// Forwards opaque packets verbatim (no encoding, sequencing, or retries) and
/// holds only the freshest pending frame via [`DataTrackSendQueue`]. A frame's
/// packets are dispatched in order and never interleaved with, or evicted in
/// favour of, another frame — partial frames are never left on the wire.
/// Not used for reliable/lossy data packets, which go through
/// `SessionInner::data_channel_task` since they need unbounded queueing,
/// retries, and per-kind sequencing.
pub struct DataChannelSender {
    /// Drop-oldest queue of whole frames waiting for the DC to drain.
    queue: DataTrackSendQueue,

    /// Packets from the frame currently being dispatched, in FIFO order.
    /// Non-empty only while a frame is still being drained to the DC.
    in_flight: VecDeque<Bytes>,

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
    /// Queue capacity in frames. Set to 1 so only the freshest pending frame
    /// is held; older queued frames are evicted on the next send.
    const QUEUE_CAPACITY: usize = 1;

    /// Creates a new sender.
    ///
    /// Returns a tuple containing the following:
    /// - The sender itself to be spawned by the caller (see [`DataChannelSender::run`]).
    /// - A cloneable queue handle for producers to push frames onto.
    pub fn new(options: DataChannelSenderOptions) -> (Self, DataTrackSendQueue) {
        let queue = DataTrackSendQueue::new(Self::QUEUE_CAPACITY);
        let (dc_event_tx, dc_event_rx) = mpsc::unbounded_channel();

        let sender = Self {
            low_buffer_threshold: options.low_buffer_threshold,
            dc: options.dc,
            queue: queue.clone(),
            in_flight: VecDeque::new(),
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
                    self.drain_in_flight();
                }
                packets = self.queue.recv(),
                    if self.in_flight.is_empty()
                        && self.buffered_amount <= self.low_buffer_threshold =>
                {
                    self.in_flight.extend(packets);
                    self.drain_in_flight();
                }
                _ = self.close_rx.changed() => break
            }
        }

        let remaining = self.queue.drain();
        if !remaining.is_empty() {
            let unsent_bytes: usize =
                remaining.iter().flat_map(|frame| frame.iter()).map(|p| p.len()).sum();
            log::info!("{} byte(s) remain in queue", unsent_bytes);
        }
        log::debug!("Send task ended for data channel '{}'", self.dc.label());
    }

    /// Dispatch as many in-flight packets as possible without exceeding the
    /// buffered-amount low threshold, so each frame's packets go out in order
    /// while still respecting DC backpressure between them.
    fn drain_in_flight(&mut self) {
        while self.buffered_amount <= self.low_buffer_threshold {
            let Some(packet) = self.in_flight.pop_front() else { break };
            self.dispatch(packet);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(byte: u8, len: usize) -> Bytes {
        Bytes::from(vec![byte; len])
    }

    fn frame(byte: u8, packet_count: usize, packet_len: usize) -> DataTrackFramePackets {
        (0..packet_count).map(|_| packet(byte, packet_len)).collect()
    }

    #[test]
    fn send_empty_frame_is_noop() {
        let q = DataTrackSendQueue::new(1);
        assert!(q.send(Vec::new()).is_none());
        assert!(q.try_pop().is_none());
    }

    #[test]
    fn send_keeps_multi_packet_frame_intact() {
        let q = DataTrackSendQueue::new(1);
        let f = frame(0xAA, 13, 16_000);
        assert!(q.send(f.clone()).is_none());
        let got = q.try_pop().expect("frame should be queued");
        assert_eq!(got.len(), 13, "all packets for the frame must remain together");
        assert!(got.iter().all(|p| p.len() == 16_000 && p[0] == 0xAA));
    }

    #[test]
    fn send_drops_oldest_whole_frame_when_full() {
        let q = DataTrackSendQueue::new(1);
        let older = frame(0x01, 4, 128);
        let newer = frame(0x02, 3, 128);

        assert!(q.send(older.clone()).is_none());

        // Pushing a second frame while at capacity evicts the entire older
        // frame, not just a prefix of it.
        let evicted = q.send(newer.clone()).expect("older frame should be evicted");
        assert_eq!(evicted.len(), older.len());
        assert!(evicted.iter().all(|p| p[0] == 0x01));

        // The queue now holds only the newer frame, fully intact.
        let got = q.try_pop().expect("newer frame should remain");
        assert_eq!(got.len(), newer.len());
        assert!(got.iter().all(|p| p[0] == 0x02));
        assert!(q.try_pop().is_none());
    }

    #[tokio::test]
    async fn recv_returns_whole_frame() {
        let q = DataTrackSendQueue::new(1);
        let f = frame(0x33, 5, 64);

        let q_send = q.clone();
        let f_sent = f.clone();
        tokio::spawn(async move {
            q_send.send(f_sent);
        });

        let got = q.recv().await;
        assert_eq!(got.len(), f.len());
        assert!(got.iter().all(|p| p[0] == 0x33));
    }
}
