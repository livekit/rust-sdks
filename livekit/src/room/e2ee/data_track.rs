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

use crate::{id::ParticipantIdentity, E2eeManager};
use bytes::Bytes;
use livekit_datatrack::internal as dt;

/// Wrapper around [`E2eeManager`] implementing [`dt::EncryptionProvider`].
#[derive(Debug)]
pub(crate) struct DataTrackEncryptionProvider {
    manager: E2eeManager,
    sender_identity: ParticipantIdentity,
}

impl DataTrackEncryptionProvider {
    pub fn new(manager: E2eeManager, sender_identity: ParticipantIdentity) -> Self {
        Self { manager, sender_identity }
    }
}

impl dt::EncryptionProvider for DataTrackEncryptionProvider {
    fn encrypt(&self, payload: bytes::Bytes) -> Result<dt::EncryptedPayload, dt::EncryptionError> {
        let key_index = self
            .manager
            .key_provider()
            .map_or(0, |kp| kp.get_latest_key_index() as u32);

        let encrypted = self
            .manager
            .encrypt_data(payload.into(), &self.sender_identity, key_index)
            .map_err(|_| dt::EncryptionError)?;

        let payload = encrypted.data.into();
        let iv = encrypted.iv.try_into().map_err(|_| dt::EncryptionError)?;
        let key_index = encrypted.key_index.try_into().map_err(|_| dt::EncryptionError)?;

        Ok(dt::EncryptedPayload { payload, iv, key_index })
    }
}

/// Wrapper around [`E2eeManager`] implementing [`dt::DecryptionProvider`].
#[derive(Debug)]
pub(crate) struct DataTrackDecryptionProvider {
    manager: E2eeManager,
}

impl DataTrackDecryptionProvider {
    pub fn new(manager: E2eeManager) -> Self {
        Self { manager }
    }
}

impl dt::DecryptionProvider for DataTrackDecryptionProvider {
    fn decrypt(
        &self,
        payload: dt::EncryptedPayload,
        sender_identity: &str,
    ) -> Result<bytes::Bytes, dt::DecryptionError> {
        let decrypted = self
            .manager
            .decrypt_data(
                payload.payload.into(),
                payload.iv.to_vec(),
                payload.key_index as u32,
                sender_identity,
            )
            .ok_or_else(|| dt::DecryptionError)?;
        Ok(Bytes::from(decrypted))
    }
}
