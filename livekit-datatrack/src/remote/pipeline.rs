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

use super::{
    depacketizer::Depacketizer,
    manager::{OutputEvent, TrackState},
};
use crate::{
    api::{DataTrackFrame, DataTrackInfo},
    dtp::Dtp,
    e2ee::{DecryptionProvider, EncryptedPayload},
    remote::depacketizer::DepacketizerFrame,
};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch};

/// Pipeline for an individual data track with an active subscription.
pub(super) struct Pipeline {
    pub depacketizer: Depacketizer,
    pub e2ee_provider: Option<Arc<dyn DecryptionProvider>>,
    pub info: Arc<DataTrackInfo>,
    pub publisher_identity: Arc<str>,
    pub state_rx: watch::Receiver<TrackState>,
    pub packet_rx: mpsc::Receiver<Dtp>,
    pub frame_tx: broadcast::Sender<DataTrackFrame>,
    pub event_out_tx: mpsc::WeakSender<OutputEvent>,
}

impl Pipeline {
    /// Run the pipeline task, consuming self.
    pub async fn run(mut self) {
        log::debug!("Task started: sid={}", self.info.sid);
        let mut state = *self.state_rx.borrow();
        while state.is_published() {
            tokio::select! {
                biased;  // State updates take priority
                _ = self.state_rx.changed() => {
                    state = *self.state_rx.borrow();
                },
                Some(dtp) = self.packet_rx.recv() => {
                    self.receive_packet(dtp);
                },
                else => break
            }
        }
        log::debug!("Task ended: sid={}", self.info.sid);
        // TODO: send unsubscribe if needed
    }

    fn receive_packet(&mut self, dtp: Dtp) {
        let Some(frame) = self.depacketizer.push(dtp) else { return };
        let Some(frame) = self.decrypt_if_needed(frame) else { return };
        _ = self.frame_tx.send(frame.into());
    }

    /// Decrypt the frame's payload if E2EE is enabled for this track.
    fn decrypt_if_needed(&self, mut frame: DepacketizerFrame) -> Option<DepacketizerFrame> {
        let Some(decryption) = &self.e2ee_provider else { return frame.into() };
        debug_assert!(self.info.uses_e2ee);

        let Some(e2ee) = frame.extensions.e2ee else {
            log::error!("Missing E2EE meta");
            return None;
        };

        let encrypted =
            EncryptedPayload { payload: frame.payload, iv: e2ee.iv, key_index: e2ee.key_index };
        frame.payload = match decryption.decrypt(encrypted, &self.publisher_identity) {
            Ok(decrypted) => decrypted,
            Err(err) => {
                log::error!("{}", err);
                return None;
            }
        };
        frame.into()
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
