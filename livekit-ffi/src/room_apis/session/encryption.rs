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

//! Thin core encryption configuration and host-owned key material; no room references.

use super::media_frames::invalid;
use super::{NativeRoomSession, NativeSessionError};
use bytes::Bytes;
use livekit::e2ee::{
    key_provider::{KeyDerivationAlgorithm, KeyProvider, KeyProviderOptions},
    E2eeOptions, EncryptionType,
};
use std::{fmt, sync::Arc};

/// Core key derivation algorithm. This wrapper performs no cryptography itself.
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum NativeKeyDerivationAlgorithm {
    Pbkdf2,
    Hkdf,
}

/// Provider settings. Defaults match Swift's shared-key AES128 fixture.
#[derive(Clone, Debug, uniffi::Record)]
pub struct NativeKeyProviderOptions {
    #[uniffi(default = true)]
    pub shared_key: bool,
    #[uniffi(default = None)]
    pub ratchet_salt: Option<Bytes>,
    #[uniffi(default = 0)]
    pub ratchet_window_size: i32,
    #[uniffi(default = -1)]
    pub failure_tolerance: i32,
    #[uniffi(default = 16)]
    pub key_ring_size: i32,
    pub key_derivation_algorithm: NativeKeyDerivationAlgorithm,
    #[uniffi(default = None)]
    pub uncrypted_magic_bytes: Option<Bytes>,
    #[uniffi(default = false)]
    pub discard_frame_when_cryptor_not_ready: bool,
    #[uniffi(default = 128)]
    pub key_size: u16,
}
impl Default for NativeKeyProviderOptions {
    fn default() -> Self {
        Self {
            shared_key: true,
            ratchet_salt: None,
            ratchet_window_size: 0,
            failure_tolerance: -1,
            key_ring_size: 16,
            key_derivation_algorithm: NativeKeyDerivationAlgorithm::Pbkdf2,
            uncrypted_magic_bytes: None,
            discard_frame_when_cryptor_not_ready: false,
            key_size: 128,
        }
    }
}

/// Host-owned core key provider. Keeping it alive never retains a room/session.
#[derive(uniffi::Object)]
pub struct NativeKeyProvider {
    pub(super) core: KeyProvider,
    shared: bool,
    ring_size: i32,
}
impl fmt::Debug for NativeKeyProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeKeyProvider").field("shared", &self.shared).finish_non_exhaustive()
    }
}
impl NativeKeyProvider {
    fn index(&self, index: i32) -> Result<(), NativeSessionError> {
        if !(0..self.ring_size).contains(&index) {
            return Err(invalid("key index is outside the key ring"));
        }
        Ok(())
    }
    fn identity(
        &self,
        identity: Option<String>,
    ) -> Result<Option<livekit::id::ParticipantIdentity>, NativeSessionError> {
        if self.shared {
            return Ok(None);
        }
        identity
            .filter(|s| !s.is_empty())
            .map(|s| Some(s.into()))
            .ok_or_else(|| invalid("participant identity is required for a non-shared key"))
    }
}
#[uniffi::export]
impl NativeKeyProvider {
    /// Shared mode requires an initial nonempty key. Core has no empty shared-provider constructor.
    /// Unsupported AES256/discard-when-not-ready settings fail explicitly, never downgrade.
    #[uniffi::constructor]
    pub fn new(
        options: NativeKeyProviderOptions,
        shared_key: Option<Bytes>,
    ) -> Result<Arc<Self>, NativeSessionError> {
        if options.key_size != 128 || options.discard_frame_when_cryptor_not_ready {
            return Err(invalid("core KeyProviderOptions exposes only AES128 and discardFrameWhenCryptorNotReady=false"));
        }
        if !(1..=255).contains(&options.key_ring_size)
            || !(0..=255).contains(&options.ratchet_window_size)
            || options.failure_tolerance < -1
        {
            return Err(invalid("invalid key ring, ratchet window or failure tolerance"));
        }
        let salt =
            options.ratchet_salt.unwrap_or_else(|| Bytes::from_static(b"LKFrameEncryptionKey"));
        if salt.is_empty() || salt.len() > 4096 {
            return Err(invalid("ratchet salt must contain 1..4096 bytes"));
        }
        let core_options = KeyProviderOptions {
            ratchet_salt: salt.to_vec(),
            ratchet_window_size: options.ratchet_window_size,
            failure_tolerance: options.failure_tolerance,
            key_ring_size: options.key_ring_size,
            key_derivation_algorithm: match options.key_derivation_algorithm {
                NativeKeyDerivationAlgorithm::Pbkdf2 => KeyDerivationAlgorithm::PBKDF2,
                NativeKeyDerivationAlgorithm::Hkdf => KeyDerivationAlgorithm::HKDF,
            },
        };
        let core = if options.shared_key {
            let key = shared_key.ok_or_else(|| invalid("shared mode requires an initial key"))?;
            validate_key(&key)?;
            KeyProvider::with_shared_key(core_options, key.to_vec())
        } else {
            if shared_key.is_some() {
                return Err(invalid("shared key supplied for non-shared mode"));
            }
            KeyProvider::new(core_options)
        };
        let provider =
            Arc::new(Self { core, shared: options.shared_key, ring_size: options.key_ring_size });
        provider.set_sif_trailer(
            options.uncrypted_magic_bytes.unwrap_or_else(|| Bytes::from_static(b"LK-ROCKS")),
        )?;
        Ok(provider)
    }
    /// Sets key material through core. Core also updates its latest data-packet key index.
    /// Existing media cryptors do not switch index; this is not Swift setCurrentKeyIndex parity.
    pub fn set_key(
        &self,
        key: Bytes,
        participant_identity: Option<String>,
        index: i32,
    ) -> Result<(), NativeSessionError> {
        self.index(index)?;
        validate_key(&key)?;
        match self.identity(participant_identity)? {
            None => self.core.set_shared_key(key.to_vec(), index),
            Some(identity) => {
                if !self.core.set_key(&identity, index, key.to_vec()) {
                    return Err(invalid("core rejected participant key"));
                }
            }
        }
        Ok(())
    }
    /// Exports original key material using core's provider API.
    pub fn export_key(
        &self,
        participant_identity: Option<String>,
        index: i32,
    ) -> Result<Option<Bytes>, NativeSessionError> {
        self.index(index)?;
        Ok(match self.identity(participant_identity)? {
            None => self.core.get_shared_key(index),
            Some(identity) => self.core.get_key(&identity, index),
        }
        .map(Bytes::from))
    }
    /// Ratchets key material through core; no algorithm is duplicated in FFI.
    pub fn ratchet_key(
        &self,
        participant_identity: Option<String>,
        index: i32,
    ) -> Result<Option<Bytes>, NativeSessionError> {
        self.index(index)?;
        Ok(match self.identity(participant_identity)? {
            None => self.core.ratchet_shared_key(index),
            Some(identity) => self.core.ratchet_key(&identity, index),
        }
        .map(Bytes::from))
    }
    /// Sets core's server-injected-frame trailer; the room may replace it on join.
    pub fn set_sif_trailer(&self, trailer: Bytes) -> Result<(), NativeSessionError> {
        if trailer.len() > 4096 {
            return Err(invalid("SIF trailer exceeds 4096 bytes"));
        }
        self.core.set_sif_trailer(trailer.to_vec());
        Ok(())
    }
    /// Core's latest key-material index; not an independent media sender-index setting.
    pub fn latest_key_index(&self) -> i32 {
        self.core.get_latest_key_index()
    }
}
fn validate_key(key: &[u8]) -> Result<(), NativeSessionError> {
    if key.is_empty() || key.len() > 65536 {
        return Err(invalid("key material must contain 1..65536 bytes"));
    }
    Ok(())
}

/// AES-GCM encryption. Default encrypts both data and media; legacy mode encrypts media only.
#[derive(Clone, Debug, uniffi::Record)]
pub struct NativeEncryptionOptions {
    pub key_provider: Arc<NativeKeyProvider>,
    #[uniffi(default = false)]
    pub legacy_media_only: bool,
}
impl NativeEncryptionOptions {
    pub(super) fn core(&self) -> E2eeOptions {
        E2eeOptions {
            encryption_type: EncryptionType::Gcm,
            key_provider: self.key_provider.core.clone(),
        }
    }
}

/// Current core encryption switches, without exposing key material.
#[derive(Clone, Debug, uniffi::Record)]
pub struct NativeEncryptionStatus {
    pub media_enabled: bool,
    pub data_enabled: bool,
}
#[uniffi::export]
impl NativeRoomSession {
    /// Reads actual room encryption status.
    pub fn encryption_status(&self) -> Result<NativeEncryptionStatus, NativeSessionError> {
        let room = self.shared.room()?;
        let manager = room.e2ee_manager();
        Ok(NativeEncryptionStatus {
            media_enabled: manager.enabled(),
            data_enabled: manager.is_dc_encryption_enabled(),
        })
    }
    /// Changes media cryptors only. Core does not expose a matching dynamic data-encryption switch.
    pub async fn set_media_encryption_enabled(
        &self,
        enabled: bool,
    ) -> Result<(), NativeSessionError> {
        let room = self.shared.room()?;
        if room.e2ee_manager().key_provider().is_none() {
            return Err(invalid("encryption was not configured at connect"));
        }
        self.shared
            .command(async move {
                room.e2ee_manager().set_enabled(enabled);
            })
            .await
    }
}
