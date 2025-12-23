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
use from_variants::FromVariants;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::Stream;
use livekit_protocol::{self as proto};

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

pub struct Manager {
    signal_in_tx: mpsc::Sender<SubSignalInput>,
    // sub request
}

impl Manager {
    pub fn new(
        options: SubManagerOptions,
    ) -> (
        Self,
        ManagerTask, /*,impl Stream<Item = SubSignalOutput>, impl Stream<Item = DataTrack<Remote>>*/
    ) {
        todo!()
    }

    /// Handles a signal message from the SFU.
    ///
    /// In order to function correctly, all message types enumerated in [`SubSignalInput`]
    /// must be forwarded here.
    ///
    pub fn handle_signal(&self, message: SubSignalInput) -> Result<(), InternalError> {
        Ok(self.signal_in_tx.try_send(message).context("Failed to handle signal input")?)
    }
}

pub struct ManagerTask {
    decryption: Option<Arc<dyn DecryptionProvider>>,
}

impl ManagerTask {
    pub async fn run(mut self) -> Result<(), InternalError> {
        Ok(())
    }
}

/// Signal message produced by [`SubManager`] to be forwarded to the SFU.
#[derive(Debug, FromVariants)]
pub enum SubSignalOutput {
    UpdateSubscription(proto::UpdateDataSubscription)
}

/// Signal message received from the SFU handled by [`SubManager`].
#[derive(Debug, FromVariants)]
pub enum SubSignalInput {
    ParticipantUpdate(proto::ParticipantUpdate),
    UnpublishResponse(proto::UnpublishDataTrackResponse),
    SubscriberHandles(proto::DataTrackSubscriberHandles)
}
