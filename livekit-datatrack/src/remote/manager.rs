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

use crate::{DataTrack, DecryptionProvider, InternalError, Remote};
use anyhow::Context;
use bytes::Bytes;
use from_variants::FromVariants;
use livekit_protocol as proto;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream};

#[derive(Debug, Clone)]
pub(crate) struct TrackInner {
    // frame_rx
    // state...
}

impl TrackInner {
    // manage subscription
}

impl Drop for TrackInner {
    fn drop(&mut self) {
        // unsubscribe
    }
}

struct TrackTask {
    // depacketizer
    // decryption
    // state_rx (from manager)
    // frame_tx (to track inner)
    // packet_in_rx
    // signal_out_tx
}

impl TrackTask {
    async fn run(mut self) -> Result<(), InternalError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct SubManagerOptions {
    pub decryption: Option<Arc<dyn DecryptionProvider>>,
}

/// Manager for remote data tracks.
pub struct Manager {
    signal_in_tx: mpsc::Sender<SubSignalInput>,
    packet_in_tx: mpsc::Sender<Bytes>,
    // sub request
}

impl Manager {
    const CH_BUFFER_SIZE: usize = 4;

    pub fn new(
        options: SubManagerOptions,
    ) -> (
        Self,
        ManagerTask,
        impl Stream<Item = SubSignalOutput>,
        impl Stream<Item = DataTrack<Remote>>,
    ) {
        let (signal_in_tx, signal_in_rx) = mpsc::channel(Self::CH_BUFFER_SIZE);
        let (signal_out_tx, signal_out_rx) = mpsc::channel(Self::CH_BUFFER_SIZE);
        let (track_out_tx, track_out_rx) = mpsc::channel(Self::CH_BUFFER_SIZE);
        let (packet_in_tx, packet_in_rx) = mpsc::channel(Self::CH_BUFFER_SIZE);

        let manager = Manager { signal_in_tx, packet_in_tx };
        let task = ManagerTask {
            decryption: options.decryption,
            signal_in_rx,
            signal_out_tx,
            track_out_tx,
            packet_in_rx
        };

        let signal_out_stream = ReceiverStream::new(signal_out_rx);
        let track_out_stream = ReceiverStream::new(track_out_rx);

        (manager, task, signal_out_stream, track_out_stream)
    }

    /// Handles a signal message from the SFU.
    ///
    /// In order to function correctly, all message types enumerated in [`SubSignalInput`]
    /// must be forwarded here.
    ///
    pub fn handle_signal(&self, message: SubSignalInput) -> Result<(), InternalError> {
        Ok(self.signal_in_tx.try_send(message).context("Failed to handle signal input")?)
    }

    /// Handles a packet received over the transport.
    pub fn handle_packet(&self, packet: Bytes) -> Result<(), InternalError> {
        Ok(self.packet_in_tx.try_send(packet).context("Failed to packet")?)
    }
}

pub struct ManagerTask {
    decryption: Option<Arc<dyn DecryptionProvider>>,
    signal_in_rx: mpsc::Receiver<SubSignalInput>,
    signal_out_tx: mpsc::Sender<SubSignalOutput>,
    track_out_tx: mpsc::Sender<DataTrack<Remote>>,
    packet_in_rx: mpsc::Receiver<Bytes>
}

impl ManagerTask {
    pub async fn run(mut self) -> Result<(), InternalError> {
        loop {
            tokio::select! {
                biased; // Handle signal input first
                // TODO: check cancellation
                Some(signal) = self.signal_in_rx.recv() => self.handle_signal(signal),
                Some(packet) = self.packet_in_rx.recv() => self.handle_packet(packet),
                else => Ok(())
            }
            .inspect_err(|err| log::error!("{}", err))
            .ok();
        }
    }

    fn handle_packet(&mut self, packet: Bytes) -> Result<(), InternalError> {
        todo!()
    }

    fn handle_signal(&mut self, message: SubSignalInput) -> Result<(), InternalError> {
        match message {
            SubSignalInput::ParticipantUpdate(message) => self.handle_participant_update(message),
            SubSignalInput::SubscriberHandles(message) => self.handle_subscriber_handles(message)
        }
    }

    fn handle_participant_update(&mut self, message: proto::ParticipantUpdate) -> Result<(), InternalError> {
        todo!()
    }

    fn handle_subscriber_handles(&mut self, message: proto::DataTrackSubscriberHandles) -> Result<(), InternalError> {
        todo!()
    }
}

/// Signal message produced by [`Manager`] to be forwarded to the SFU.
#[derive(Debug, FromVariants)]
pub enum SubSignalOutput {
    UpdateSubscription(proto::UpdateDataSubscription),
}

/// Signal message received from the SFU handled by [`Manager`].
#[derive(Debug, FromVariants)]
pub enum SubSignalInput {
    ParticipantUpdate(proto::ParticipantUpdate),
    SubscriberHandles(proto::DataTrackSubscriberHandles),
}
