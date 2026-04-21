// Copyright 2026 LiveKit, Inc.
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

use bytes::Bytes;
use core::fmt;
use livekit_datatrack::backend::{
    DecryptionError, DecryptionProvider, EncryptedPayload, EncryptionError, EncryptionProvider,
    InitializationVector,
};
use std::sync::Arc;

#[uniffi::remote(Error)]
#[uniffi(flat_error)]
pub enum EncryptionError {}

#[uniffi::remote(Error)]
#[uniffi(flat_error)]
pub enum DecryptionError {}

#[uniffi::remote(Record)]
pub struct EncryptedPayload {
    pub payload: Bytes,
    pub iv: InitializationVector,
    pub key_index: u8,
}

uniffi::custom_type!(InitializationVector, Vec<u8>, {
    remote,
    lower: |iv| iv.to_vec(),
    try_lift: |v| v.try_into()
        .map_err(|_| uniffi::deps::anyhow::anyhow!("IV must be exactly 12 bytes"))
});

/// Provider for encrypting payloads for E2EE.
#[uniffi::export(with_foreign)]
pub trait DataTrackEncryptionProvider: Send + Sync {
    /// Encrypts the given payload being sent by the local participant.
    fn encrypt(&self, payload: Bytes) -> Result<EncryptedPayload, EncryptionError>;
}

/// Provider for decrypting payloads for E2EE.
#[uniffi::export(with_foreign)]
pub trait DataTrackDecryptionProvider: Send + Sync {
    /// Decrypts the given payload received from a remote participant.
    ///
    /// Sender identity is required in order for the proper key to be used
    /// for decryption.
    ///
    fn decrypt(
        &self,
        payload: EncryptedPayload,
        sender_identity: String,
    ) -> Result<Bytes, DecryptionError>;
}

/// Adapts [`DataTrackEncryptionProvider`] to implement [`EncryptionProvider`].
pub(super) struct FfiEncryptionProvider(pub(super) Arc<dyn DataTrackEncryptionProvider>);

impl EncryptionProvider for FfiEncryptionProvider {
    fn encrypt(&self, payload: Bytes) -> Result<EncryptedPayload, EncryptionError> {
        self.0.encrypt(payload)
    }
}

impl fmt::Debug for FfiEncryptionProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FfiEncryptionProvider").finish()
    }
}

/// Adapts [`DataTrackDecryptionProvider`] to implement [`DecryptionProvider`].
pub(super) struct FfiDecryptionProvider(pub(super) Arc<dyn DataTrackDecryptionProvider>);

impl DecryptionProvider for FfiDecryptionProvider {
    fn decrypt(
        &self,
        payload: EncryptedPayload,
        sender_identity: &str,
    ) -> Result<Bytes, DecryptionError> {
        self.0.decrypt(payload, sender_identity.to_string())
    }
}

impl fmt::Debug for FfiDecryptionProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FfiDecryptionProvider").finish()
    }
}
