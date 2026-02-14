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
use tokio::sync::{mpsc, watch};

/// Options for constructing a [`DataChannelSender`].
pub struct DataChannelSenderOptions {
    pub low_buffer_threshold: u64,
    pub dc: DataChannel,
    pub close_rx: watch::Receiver<bool>,
}

/// A task responsible for sending payloads over a single RTC data channel.
///
/// This implements the same backpressure logic used by `SessionInner::data_channel_task`,
/// but is decoupled from message encoding and retry logic specific to data packets.
///
/// It was originally introduced to support sending data track packets; however, the
/// implementation is generic and works with arbitrary payloads.
///
/// In a future refactor, it would be worth revisiting how the logic in
/// `SessionInner::data_channel_task` can be decoupled from session likely by reusing this
/// sender and moving encoding and retry concerns into a separate layer
/// (see the `livekit-datatrack` crate for an example of this approach).
///
pub struct DataChannelSender {
    /// Channel for receiving payloads to be enqueued for sending.
    send_rx: mpsc::Receiver<Bytes>,

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

    /// Payloads enqueued for sending.
    send_queue: VecDeque<Bytes>,
}

impl DataChannelSender {
    /// Creates a new sender.
    ///
    /// Returns a tuple containing the following:
    /// - The sender itself to be spawned by the caller (see [`DataChannelSender::run`]).
    /// - Channel for sending payloads over the data channel.
    ///
    pub fn new(options: DataChannelSenderOptions) -> (Self, mpsc::Sender<Bytes>) {
        let (send_tx, send_rx) = mpsc::channel(128);
        let (dc_event_tx, dc_event_rx) = mpsc::unbounded_channel();

        let sender = Self {
            low_buffer_threshold: options.low_buffer_threshold,
            dc: options.dc,
            send_rx,
            dc_event_rx,
            dc_event_tx,
            close_rx: options.close_rx,
            buffered_amount: 0,
            send_queue: VecDeque::default(),
        };
        (sender, send_tx)
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
                Some(payload) = self.send_rx.recv() => {
                    self.handle_enqueue_for_send(payload)
                }
                _ = self.close_rx.changed() => break
            }
        }

        if !self.send_queue.is_empty() {
            let unsent_bytes: usize =
                self.send_queue.into_iter().map(|payload| payload.len()).sum();
            log::info!("{} byte(s) remain in queue", unsent_bytes);
        }
        log::debug!("Send task ended for data channel '{}'", self.dc.label());
    }

    fn send_until_threshold(&mut self) {
        while self.buffered_amount <= self.low_buffer_threshold {
            let Some(payload) = self.send_queue.pop_front() else {
                break;
            };
            self.buffered_amount += payload.len() as u64;
            _ = self
                .dc
                .send(&payload, true)
                .inspect_err(|err| log::error!("Failed to send data: {}", err));
        }
    }

    fn handle_enqueue_for_send(&mut self, payload: Bytes) {
        self.send_queue.push_back(payload);
        self.send_until_threshold();
    }

    fn handle_bytes_sent(&mut self, bytes_sent: u64) {
        if self.buffered_amount < bytes_sent {
            log::error!("Unexpected buffer amount");
            self.buffered_amount = 0;
            return;
        }
        self.buffered_amount -= bytes_sent;
        self.send_until_threshold();
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
