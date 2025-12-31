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

use crate::{
    dtp,
    local::manager::{LocalTrackState, OutputEvent, UnpublishInitiator, UnpublishRequestEvent},
    DataTrackFrame, DataTrackInfo, EncryptionProvider, InternalError,
};
use anyhow::Context;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

/// Task responsible for operating an individual published data track.
pub(super) struct LocalTrackTask {
    pub packetizer: dtp::Packetizer,
    pub encryption: Option<Arc<dyn EncryptionProvider>>,
    pub info: Arc<DataTrackInfo>,
    pub state_rx: watch::Receiver<LocalTrackState>,
    pub frame_rx: mpsc::Receiver<DataTrackFrame>,
    pub event_out_tx: mpsc::Sender<OutputEvent>,
}

impl LocalTrackTask {
    pub async fn run(mut self) {
        let mut state = *self.state_rx.borrow();
        while state.is_published() {
            tokio::select! {
                biased;
                _ = self.state_rx.changed() => {
                    state = *self.state_rx.borrow();
                },
                Some(frame) = self.frame_rx.recv() => {
                    _ = self.publish_frame(frame).inspect_err(|err| log::error!("{}", err));
                },
                else => break
            }
        }
        if let LocalTrackState::Unpublished { initiator: UnpublishInitiator::Client } = state {
            let event = UnpublishRequestEvent { handle: self.info.handle };
            _ = self.event_out_tx.try_send(event.into());
        }
    }

    fn publish_frame(&mut self, mut frame: DataTrackFrame) -> Result<(), InternalError> {
        let mut e2ee: Option<dtp::E2ee> = None;
        if let Some(encryption) = &self.encryption {
            debug_assert!(self.info.uses_e2ee);
            let encrypted_payload =
                encryption.encrypt(frame.payload).context("Failed to encrypt frame")?;
            e2ee = Some(dtp::E2ee {
                key_index: encrypted_payload.key_index,
                iv: encrypted_payload.iv,
            });
            frame.payload = encrypted_payload.payload;
        }

        let frame = dtp::PacketizerFrame {
            payload: frame.payload,
            e2ee,
            user_timestamp: frame.user_timestamp,
        };
        let packets = self.packetizer.packetize(frame).context("Failed to packetize frame")?;
        for packet in packets {
            let serialized = packet.serialize();
            self.event_out_tx.try_send(serialized.into()).context("Failed to send packet")?;
        }
        Ok(())
    }
}
