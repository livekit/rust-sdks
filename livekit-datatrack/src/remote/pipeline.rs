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

use super::manager::{TrackState, TrackSubscriptionEvent};
use crate::{DataTrackFrame, DataTrackInfo, DecryptionProvider, InternalError, dtp::Dtp};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch};

// pub enum RemoteTrackState {
//     /// Track available to be subscribed to.
//     Published,
//     /// Local participant subscribed to the track.
//     Subscribed,
//     /// Track has been unpublished and is no longer available.
//     Unpublished,
// }

// update_subscription_tx
// state_rx




pub(super) struct RemoteTrackTask {
    // pub depacketizer: dtp::Depacketizer,
    pub decryption: Option<Arc<dyn DecryptionProvider>>,
    pub info: Arc<DataTrackInfo>,
    //pub subscription_rx: mpsc::Receiver<>
    pub state_rx: watch::Receiver<TrackState>,
    pub subscription_rx: mpsc::Receiver<TrackSubscriptionEvent>,
    pub packet_rx: mpsc::Receiver<Dtp>,
    pub frame_tx: broadcast::Sender<DataTrackFrame>,
    pub event_out_tx: mpsc::Sender<super::manager::OutputEvent>,
    // TODO: mechanism to update subscription?
}

impl RemoteTrackTask {
    pub async fn run(mut self) -> Result<(), InternalError> {
        let mut state = *self.state_rx.borrow();
        while state.is_published() {
            tokio::select! {
                biased;
                _ = self.state_rx.changed() => {
                    state = *self.state_rx.borrow();
                },
                // Some(frame) = self.frame_rx.recv() => {
                //     _ = self.publish_frame(frame).inspect_err(|err| log::error!("{}", err));
                // },
                else => break
            }
        }
        Ok(())
    }
}
