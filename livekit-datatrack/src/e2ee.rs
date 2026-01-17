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

/// Encrypted payload and metadata required for decryption.
pub struct EncryptedPayload {
    pub payload: Bytes,
    pub iv: [u8; 12],
    pub key_index: u8,
}

#[derive(Debug, Error)]
#[error("Encryption failed")]
pub struct EncryptionError;

pub trait EncryptionProvider: Send + Sync + Debug {
    /// Encrypts the given payload being sent by the local participant.
    fn encrypt(&self, payload: Bytes) -> Result<EncryptedPayload, EncryptionError>;
}

#[derive(Debug, Error)]
#[error("Decryption failed")]
pub struct DecryptionError;

pub trait DecryptionProvider: Send + Sync + Debug {
    /// Decrypts the given payload received from a remote participant.
    fn decrypt(
        &self,
        payload: EncryptedPayload,
        sender_identity: &str,
    ) -> Result<Bytes, DecryptionError>;
}
