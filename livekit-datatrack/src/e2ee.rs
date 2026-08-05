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

use bytes::Bytes;
use core::fmt::Debug;
use thiserror::Error;

// TODO: If a core module for end-to-end encryption is created in the future
// (livekit-e2ee), these traits should be moved to there.

/// Twelve byte AES initialization vector (IV).
pub type InitializationVector = [u8; 12];

/// Encrypted payload and metadata required for decryption.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct EncryptedPayload {
    pub payload: Bytes,
    pub iv: InitializationVector,
    pub key_index: u8,
}

/// An error indicating a payload could not be encrypted.
#[derive(Debug, Error)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Error))]
#[cfg_attr(feature = "uniffi", uniffi(flat_error))]
pub enum EncryptionError {
    #[error("Encryption failed")]
    Failed,
}

/// An error indicating a payload could not be decrypted.
#[derive(Debug, Error)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Error))]
#[cfg_attr(feature = "uniffi", uniffi(flat_error))]
pub enum DecryptionError {
    #[error("Decryption failed")]
    Failed,
}

/// Provider for encrypting payloads for E2EE.
#[cfg_attr(feature = "uniffi", uniffi::export(with_foreign))]
pub trait EncryptionProvider: Send + Sync + Debug {
    /// Encrypts the given payload being sent by the local participant.
    fn encrypt(&self, payload: Bytes) -> Result<EncryptedPayload, EncryptionError>;
}

/// Provider for decrypting payloads for E2EE.
#[cfg_attr(feature = "uniffi", uniffi::export(with_foreign))]
pub trait DecryptionProvider: Send + Sync + Debug {
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

    // TODO: the above method previously took &str for sender_identity but has
    // been modified to accept String so it can be exported for UniFFI. However,
    // this results in an unnecessary heap allocation when used in a Rust-only context.
    // Find a better solution for this.
}

#[cfg(feature = "uniffi")]
uniffi::custom_type!(Bytes, Vec<u8>, { remote });

#[cfg(feature = "uniffi")]
uniffi::custom_type!(InitializationVector, Vec<u8>, {
    remote,
    lower: |iv| iv.to_vec(),
    try_lift: |v| v.try_into()
        .map_err(|_| uniffi::deps::anyhow::anyhow!("IV must be exactly 12 bytes"))
});
