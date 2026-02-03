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
    e2ee::{EncryptionError, EncryptionProvider},
    local::packetizer::PacketizerError,
    packet::{self, Extensions, Packet, UserTimestampExt},
};
use from_variants::FromVariants;
use std::sync::Arc;
use thiserror::Error;
/// Options for creating a [`Pipeline`].
pub(super) struct PipelineOptions {
    pub info: Arc<DataTrackInfo>,
    pub encryption_provider: Option<Arc<dyn EncryptionProvider>>,
}

/// Pipeline for an individual published data track.
pub(super) struct Pipeline {
    encryption_provider: Option<Arc<dyn EncryptionProvider>>,
    packetizer: Packetizer,
}

#[derive(Debug, Error, FromVariants)]
pub(super) enum PipelineError {
    #[error(transparent)]
    Packetizer(PacketizerError),
    #[error(transparent)]
    Encryption(EncryptionError),
}

impl Pipeline {
    /// Creates a new pipeline with the given options.
    pub fn new(options: PipelineOptions) -> Self {
        debug_assert_eq!(options.info.uses_e2ee, options.encryption_provider.is_some());
        let packetizer = Packetizer::new(options.info.pub_handle, Self::TRANSPORT_MTU);
        Self { encryption_provider: options.encryption_provider, packetizer }
    }

    pub fn process_frame(&mut self, frame: DataTrackFrame) -> Result<Vec<Packet>, PipelineError> {
        let frame = self.encrypt_if_needed(frame.into())?;
        let packets = self.packetizer.packetize(frame)?;
        Ok(packets)
    }

    /// Encrypt the frame's payload if E2EE is enabled for this track.
    fn encrypt_if_needed(
        &self,
        mut frame: PacketizerFrame,
    ) -> Result<PacketizerFrame, EncryptionError> {
        let Some(e2ee_provider) = &self.encryption_provider else {
            return Ok(frame.into());
        };

        let encrypted = e2ee_provider.encrypt(frame.payload)?;

        frame.payload = encrypted.payload;
        frame.extensions.e2ee =
            packet::E2eeExt { key_index: encrypted.key_index, iv: encrypted.iv }.into();
        Ok(frame)
    }

    /// Maximum transmission unit (MTU) of the transport.
    const TRANSPORT_MTU: usize = 16_000;
}

impl From<DataTrackFrame> for PacketizerFrame {
    fn from(frame: DataTrackFrame) -> Self {
        Self {
            payload: frame.payload,
            extensions: Extensions {
                user_timestamp: frame.user_timestamp.map(UserTimestampExt),
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

    #[test]
    fn test_process_frame() {
        let mut info: DataTrackInfo = Faker.fake();
        info.uses_e2ee = false;

        let options = PipelineOptions { info: info.into(), encryption_provider: None };
        let mut pipeline = Pipeline::new(options);

        let repeated_byte: u8 = Faker.fake();
        let frame = DataTrackFrame {
            payload: Bytes::from(vec![repeated_byte; 32_000]),
            user_timestamp: Faker.fake(),
        };

        let packets = pipeline.process_frame(frame).unwrap();
        assert_eq!(packets.len(), 3);

        for packet in packets {
            assert!(packet.header.extensions.e2ee.is_none());
            assert!(!packet.payload.is_empty());
            assert!(packet.payload.iter().all(|byte| *byte == repeated_byte));
        }
    }
}
