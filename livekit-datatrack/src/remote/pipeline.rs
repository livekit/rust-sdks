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

use super::manager::TrackState;
use crate::{dtp::Dtp, DataTrackFrame, DataTrackInfo, DecryptionProvider, EncryptedPayload};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch};

pub(super) struct RemoteTrackTask {
    // pub depacketizer: dtp::Depacketizer,
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

    async fn receive_packet(&mut self, mut dtp: Dtp) {
        if let Some(decryption) = &self.decryption {
            debug_assert!(self.info.uses_e2ee);

            let Some(e2ee_meta) = dtp.header.e2ee else {
                log::error!("Missing E2EE meta");
                return;
            };
            let encrypted_payload = EncryptedPayload {
                payload: dtp.payload,
                iv: e2ee_meta.iv,
                key_index: e2ee_meta.key_index,
            };
            let decrypted_payload = match decryption.decrypt(encrypted_payload) {
                Ok(decrypted_payload) => decrypted_payload,
                Err(err) => {
                    log::error!("Decryption failed: {}", err);
                    return;
                }
            };
            dtp.payload = decrypted_payload;
        }
        // TODO: depacketize, emit complete frame
    }
}
