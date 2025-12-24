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

use super::manager::PubSignalOutput;
use crate::{
    dtp, DataTrackFrame, DataTrackInfo, DataTrackState, EncryptionProvider, InternalError,
    PublishFrameError, PublishFrameErrorReason,
};
use anyhow::Context;
use bytes::Bytes;
use livekit_protocol as proto;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

#[derive(Debug, Clone)]
pub(crate) struct LocalTrackInner {
    pub frame_tx: mpsc::Sender<DataTrackFrame>,
    pub state_tx: watch::Sender<DataTrackState>,
}

impl LocalTrackInner {
    pub fn publish(&self, frame: DataTrackFrame) -> Result<(), PublishFrameError> {
        if !self.is_published() {
            return Err(PublishFrameError::new(frame, PublishFrameErrorReason::TrackUnpublished));
        }
        self.frame_tx.try_send(frame).map_err(|err| {
            PublishFrameError::new(err.into_inner(), PublishFrameErrorReason::Dropped)
        })
    }

    pub fn is_published(&self) -> bool {
        matches!(*self.state_tx.borrow(), DataTrackState::Published)
    }

    pub fn unpublish(&self) {
        self.state_tx
            .send(DataTrackState::Unpublished { sfu_initiated: false })
            .inspect_err(|err| log::error!("Failed to update state to unsubscribed: {err}"))
            .ok();
    }
}

impl Drop for LocalTrackInner {
    fn drop(&mut self) {
        // Implicit unpublish when handle dropped.
        self.unpublish();
    }
}

/// Task responsible for operating an individual published data track.
pub(super) struct LocalTrackTask {
    // TODO: packetizer, e2ee_provider, rate tracking, etc.
    pub packetizer: dtp::Packetizer,
    pub encryption: Option<Arc<dyn EncryptionProvider>>,
    pub info: Arc<DataTrackInfo>,
    pub state_rx: watch::Receiver<DataTrackState>,
    pub frame_rx: mpsc::Receiver<DataTrackFrame>,
    pub packet_out_tx: mpsc::Sender<Bytes>,
    pub signal_out_tx: mpsc::Sender<PubSignalOutput>,
}

impl LocalTrackTask {
    pub async fn run(mut self) -> Result<(), InternalError> {
        let mut state = DataTrackState::Published;
        while matches!(state, DataTrackState::Published) {
            tokio::select! {
                _ = self.state_rx.changed() => {
                    let _: () = state = *self.state_rx.borrow();
                    Ok(())
                },
                Some(frame) = self.frame_rx.recv() => self.publish_frame(frame),
                else => break
            }
            .inspect_err(|err| log::error!("{}", err))
            .ok();
        }
        if let DataTrackState::Unpublished { sfu_initiated } = state {
            if !sfu_initiated {
                self.send_unpublish_req()?;
            }
        }
        Ok(())
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
            self.packet_out_tx.try_send(serialized).context("Failed to send packet")?;
        }
        Ok(())
    }

    fn send_unpublish_req(self) -> Result<(), InternalError> {
        let req = proto::UnpublishDataTrackRequest { pub_handle: self.info.handle.into() };
        Ok(self.signal_out_tx.try_send(req.into()).context("Failed to send unpublish")?)
    }
}
