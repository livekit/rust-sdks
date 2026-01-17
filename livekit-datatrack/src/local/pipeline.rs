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

use super::packetizer::{Packetizer, PacketizerFrame};
use crate::{
    api::{DataTrackFrame, DataTrackInfo},
    dtp::{self, Extensions, UserTimestampExt},
    e2ee::EncryptionProvider,
    local::manager::{LocalTrackState, OutputEvent, UnpublishInitiator, UnpublishRequestEvent},
};
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

/// Pipeline for an individual published data track.
pub(super) struct Pipeline {
    pub packetizer: Packetizer,
    pub e2ee_provider: Option<Arc<dyn EncryptionProvider>>,
    pub info: Arc<DataTrackInfo>,
    pub state_rx: watch::Receiver<LocalTrackState>,
    pub frame_rx: mpsc::Receiver<DataTrackFrame>,
    pub event_out_tx: mpsc::Sender<OutputEvent>,
}

impl Pipeline {
    /// Run the pipeline task, consuming self.
    pub async fn run(mut self) {
        log::debug!("Pipeline task started: sid={}", self.info.sid);
        let mut state = *self.state_rx.borrow();
        while state.is_published() {
            tokio::select! {
                biased;  // State updates take priority
                _ = self.state_rx.changed() => {
                    state = *self.state_rx.borrow();
                },
                Some(frame) = self.frame_rx.recv() => {
                    self.publish_frame(frame);
                },
                else => break
            }
        }
        if let LocalTrackState::Unpublished { initiator: UnpublishInitiator::Client } = state {
            let event = UnpublishRequestEvent { handle: self.info.pub_handle };
            _ = self.event_out_tx.try_send(event.into());
        }
        log::debug!("Pipeline task ended: sid={}", self.info.sid);
    }

    fn publish_frame(&mut self, mut frame: DataTrackFrame) {
        let mut e2ee: Option<dtp::E2eeExt> = None;
        if let Some(e2ee_provider) = &self.e2ee_provider {
            debug_assert!(self.info.uses_e2ee);
            let encrypted_payload = match e2ee_provider.encrypt(frame.payload) {
                Ok(payload) => payload,
                Err(err) => {
                    log::error!("{}", err);
                    return;
                }
            };
            e2ee = Some(dtp::E2eeExt {
                key_index: encrypted_payload.key_index,
                iv: encrypted_payload.iv,
            });
            frame.payload = encrypted_payload.payload;
        }

        let frame = PacketizerFrame {
            payload: frame.payload,
            extensions: Extensions {
                e2ee,
                user_timestamp: frame.user_timestamp.map(|v| UserTimestampExt(v)),
            },
        };

        let packets = match self.packetizer.packetize(frame) {
            Ok(packets) => packets,
            Err(err) => {
                log::error!("Failed to packetize frame: {}", err);
                return;
            }
        };

        let packets: Vec<_> = packets.into_iter().map(|dtp| dtp.serialize()).collect();
        _ = self
            .event_out_tx
            .try_send(packets.into())
            .inspect_err(|err| log::debug!("Cannot send packet to transport: {}", err));
    }
}
