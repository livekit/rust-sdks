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

use super::{depacketizer::Depacketizer, manager::TrackState};
use crate::{
    api::{DataTrackFrame, DataTrackInfo},
    dtp::Dtp,
    e2ee::{DecryptionProvider, EncryptedPayload},
    remote::depacketizer::DepacketizerFrame,
};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch};

/// Task responsible for receiving frames for a subscribed data track.
pub(super) struct RemoteTrackTask {
    pub depacketizer: Depacketizer,
    pub decryption: Option<Arc<dyn DecryptionProvider>>,
    pub info: Arc<DataTrackInfo>,
    pub state_rx: watch::Receiver<TrackState>,
    pub packet_rx: mpsc::Receiver<Dtp>,
    pub frame_tx: broadcast::Sender<DataTrackFrame>,
    pub event_out_tx: mpsc::WeakSender<super::manager::OutputEvent>,
}

impl RemoteTrackTask {
    pub async fn run(mut self) {
        let mut state = *self.state_rx.borrow();
        while state.is_published() {
            tokio::select! {
                biased;
                _ = self.state_rx.changed() => {
                    state = *self.state_rx.borrow();
                },
                Some(dtp) = self.packet_rx.recv() => {
                    self.receive_packet(dtp);
                },
                else => break
            }
        }
        // TODO: send unsubscribe if needed
    }

    fn receive_packet(&mut self, dtp: Dtp) {
        let Some(mut frame) = self.depacketizer.push(dtp) else { return };
        if let Some(decryption) = &self.decryption {
            debug_assert!(self.info.uses_e2ee);

            let Some(e2ee) = frame.extensions.e2ee else {
                log::error!("Missing E2EE meta");
                return;
            };
            let encrypted_payload =
                EncryptedPayload { payload: frame.payload, iv: e2ee.iv, key_index: e2ee.key_index };
            let decrypted_payload = match decryption.decrypt(encrypted_payload) {
                Ok(decrypted_payload) => decrypted_payload,
                Err(err) => {
                    log::error!("Decryption failed: {}", err);
                    return;
                }
            };
            frame.payload = decrypted_payload;
        }
        _ = self.frame_tx.send(frame.into());
    }
}

impl From<DepacketizerFrame> for DataTrackFrame {
    fn from(frame: DepacketizerFrame) -> Self {
        Self {
            payload: frame.payload,
            user_timestamp: frame.extensions.user_timestamp.map(|v| v.0),
        }
    }
}
