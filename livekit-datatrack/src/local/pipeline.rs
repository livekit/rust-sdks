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
    packet::{self, Extensions, UserTimestampExt},
    e2ee::EncryptionProvider,
    local::manager::{LocalTrackState, OutputEvent, UnpublishInitiator, UnpublishRequestEvent},
};
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

/// Options for creating a [`Pipeline`].
pub(super) struct PipelineOptions {
    pub e2ee_provider: Option<Arc<dyn EncryptionProvider>>,
    pub info: Arc<DataTrackInfo>,
    pub state_rx: watch::Receiver<LocalTrackState>,
    pub frame_rx: mpsc::Receiver<DataTrackFrame>,
    pub event_out_tx: mpsc::WeakSender<OutputEvent>,
}

/// Pipeline for an individual published data track.
pub(super) struct Pipeline {
    packetizer: Packetizer,
    e2ee_provider: Option<Arc<dyn EncryptionProvider>>,
    info: Arc<DataTrackInfo>,
    state_rx: watch::Receiver<LocalTrackState>,
    frame_rx: mpsc::Receiver<DataTrackFrame>,
    event_out_tx: mpsc::WeakSender<OutputEvent>,
}

impl Pipeline {
    /// Creates a new pipeline with the given options.
    pub fn new(options: PipelineOptions) -> Self {
        debug_assert_eq!(options.info.uses_e2ee, options.e2ee_provider.is_some());
        let packetizer = Packetizer::new(options.info.pub_handle, Self::TRANSPORT_MTU);
        Self {
            packetizer,
            e2ee_provider: options.e2ee_provider,
            info: options.info,
            state_rx: options.state_rx,
            frame_rx: options.frame_rx,
            event_out_tx: options.event_out_tx,
        }
    }

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
            if let Some(event_out_tx) = self.event_out_tx.upgrade() {
                _ = event_out_tx.try_send(event.into());
            }
        }
        log::debug!("Pipeline task ended: sid={}", self.info.sid);
    }

    fn publish_frame(&mut self, frame: DataTrackFrame) {
        let Some(frame) = self.encrypt_if_needed(frame.into()) else { return };

        let packets = match self.packetizer.packetize(frame) {
            Ok(packets) => packets,
            Err(err) => {
                log::error!("Failed to packetize frame: {}", err);
                return;
            }
        };
        let packets: Vec<_> = packets.into_iter().map(|packet| packet.serialize()).collect();
        if let Some(event_out_tx) = self.event_out_tx.upgrade() {
            _ = event_out_tx
                .try_send(packets.into())
                .inspect_err(|err| log::debug!("Cannot send packet to transport: {}", err));
        }
    }

    /// Encrypt the frame's payload if E2EE is enabled for this track.
    fn encrypt_if_needed(&self, mut frame: PacketizerFrame) -> Option<PacketizerFrame> {
        let Some(e2ee_provider) = &self.e2ee_provider else { return frame.into() };

        let encrypted = match e2ee_provider.encrypt(frame.payload) {
            Ok(payload) => payload,
            Err(err) => {
                log::error!("{}", err);
                return None;
            }
        };

        frame.payload = encrypted.payload;
        frame.extensions.e2ee =
            packet::E2eeExt { key_index: encrypted.key_index, iv: encrypted.iv }.into();
        frame.into()
    }

    /// Maximum transmission unit (MTU) of the transport.
    const TRANSPORT_MTU: usize = 16_000;
}

impl From<DataTrackFrame> for PacketizerFrame {
    fn from(frame: DataTrackFrame) -> Self {
        Self {
            payload: frame.payload,
            extensions: Extensions {
                user_timestamp: frame.user_timestamp.map(|v| UserTimestampExt(v)),
                e2ee: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use fake::{Fake, Faker};

    fn make_pipeline(
    ) -> (watch::Sender<LocalTrackState>, mpsc::Sender<DataTrackFrame>, mpsc::Receiver<OutputEvent>)
    {
        let (state_tx, state_rx) = watch::channel(LocalTrackState::Published);
        let (frame_tx, frame_rx) = mpsc::channel(32);
        let (event_out_tx, event_out_rx) = mpsc::channel(32);

        let mut info: DataTrackInfo = Faker.fake();
        info.uses_e2ee = false;

        let options = PipelineOptions {
            e2ee_provider: None,
            info: info.into(),
            state_rx,
            frame_rx,
            event_out_tx: event_out_tx.downgrade(),
        };
        let pipeline = Pipeline::new(options);
        livekit_runtime::spawn(pipeline.run());

        (state_tx, frame_tx, event_out_rx)
    }

    #[tokio::test]
    async fn test_publish_frame() {
        let (_, frame_tx, mut event_out_rx) = make_pipeline();

        let frame = DataTrackFrame {
            payload: Bytes::from(vec![0xFA; 256]),
            user_timestamp: Faker.fake()
        };
        frame_tx.send(frame).await.unwrap();

        while let Some(out_event) = event_out_rx.recv().await {
            let OutputEvent::PacketsAvailable(packets) = out_event else {
                panic!("Unexpected event")
            };
            let Some(packet) = packets.first() else {
                panic!("Expected one packet")
            };
            assert!(!packet.is_empty());
            break;
        }
    }
}
