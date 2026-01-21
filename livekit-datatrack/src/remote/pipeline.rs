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

use super::depacketizer::{Depacketizer, DepacketizerFrame};
use crate::{
    api::{DataTrackFrame, DataTrackInfo},
    e2ee::{DecryptionProvider, EncryptedPayload},
    packet::Packet,
};
use std::sync::Arc;

/// Options for creating a [`Pipeline`].
pub(super) struct PipelineOptions {
    pub info: Arc<DataTrackInfo>,
    pub publisher_identity: Arc<str>,
    pub e2ee_provider: Option<Arc<dyn DecryptionProvider>>,
}

/// Pipeline for an individual data track subscription.
pub(super) struct Pipeline {
    publisher_identity: Arc<str>,
    e2ee_provider: Option<Arc<dyn DecryptionProvider>>,
    depacketizer: Depacketizer,
}

impl Pipeline {
    /// Creates a new pipeline with the given options.
    pub fn new(options: PipelineOptions) -> Self {
        debug_assert_eq!(options.info.uses_e2ee, options.e2ee_provider.is_some());
        let depacketizer = Depacketizer::new();
        Self {
            publisher_identity: options.publisher_identity,
            e2ee_provider: options.e2ee_provider,
            depacketizer,
        }
    }

    pub fn process_packet(&mut self, packet: Packet) -> Option<DataTrackFrame> {
        let Some(frame) = self.depacketizer.push(packet) else { return None };
        let Some(frame) = self.decrypt_if_needed(frame) else { return None };
        Some(frame.into())
    }

    /// Decrypt the frame's payload if E2EE is enabled for this track.
    fn decrypt_if_needed(&self, mut frame: DepacketizerFrame) -> Option<DepacketizerFrame> {
        let Some(decryption) = &self.e2ee_provider else { return frame.into() };

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
