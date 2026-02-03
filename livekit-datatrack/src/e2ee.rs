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

/// Encrypted payload and metadata required for decryption.
pub struct EncryptedPayload {
    pub payload: Bytes,
    pub iv: [u8; 12],
    pub key_index: u8,
}

/// An error indicating a payload could not be encrypted.
#[derive(Debug, Error)]
#[error("Encryption failed")]
pub struct EncryptionError;

/// An error indicating a payload could not be decrypted.
#[derive(Debug, Error)]
#[error("Decryption failed")]
pub struct DecryptionError;

/// Provider for encrypting payloads for E2EE.
pub trait EncryptionProvider: Send + Sync + Debug {
    /// Encrypts the given payload being sent by the local participant.
    fn encrypt(&self, payload: Bytes) -> Result<EncryptedPayload, EncryptionError>;
}

/// Provider for decrypting payloads for E2EE.
pub trait DecryptionProvider: Send + Sync + Debug {
    /// Decrypts the given payload received from a remote participant.
    ///
    /// Sender identity is required in order for the proper key to be used
    /// for decryption.
    ///
    fn decrypt(
        &self,
        payload: EncryptedPayload,
        sender_identity: &str,
    ) -> Result<Bytes, DecryptionError>;
}
