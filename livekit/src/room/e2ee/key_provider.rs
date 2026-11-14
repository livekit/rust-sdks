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

use libwebrtc::native::frame_cryptor as fc;
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicI32, Ordering},
        Arc, Weak,
    },
};

use crate::id::ParticipantIdentity;

pub use libwebrtc::native::frame_cryptor::KeyDerivationAlgorithm;

pub const DEFAULT_RATCHET_SALT: &str = "LKFrameEncryptionKey";
pub const DEFAULT_RATCHET_WINDOW_SIZE: i32 = 16;
pub const DEFAULT_FAILURE_TOLERANCE: i32 = -1; // no tolerance by default
pub const DEFAULT_KEY_RING_SIZE: i32 = 16;
pub const MAX_KEY_RING_SIZE: i32 = fc::MAX_KEY_RING_SIZE;
pub const DEFAULT_KEY_DERIVATION_ALGORITHM: KeyDerivationAlgorithm = KeyDerivationAlgorithm::PBKDF2;

#[derive(Clone)]
pub struct KeyProviderOptions {
    pub ratchet_window_size: i32,
    pub ratchet_salt: Vec<u8>,
    pub failure_tolerance: i32,
    pub key_ring_size: i32,
    pub key_derivation_algorithm: KeyDerivationAlgorithm,
}

impl Default for KeyProviderOptions {
    fn default() -> Self {
        Self {
            ratchet_window_size: DEFAULT_RATCHET_WINDOW_SIZE,
            ratchet_salt: DEFAULT_RATCHET_SALT.to_owned().into_bytes(),
            failure_tolerance: DEFAULT_FAILURE_TOLERANCE,
            key_ring_size: DEFAULT_KEY_RING_SIZE,
            key_derivation_algorithm: DEFAULT_KEY_DERIVATION_ALGORITHM,
        }
    }
}

/// Called with the identity a key was set for, or `None` for a shared key, and the key's index.
pub(crate) type KeySetListener = dyn Fn(Option<&ParticipantIdentity>, i32) + Send + Sync;

/// Which key each participant's media is to be encrypted with, and who wants to know.
#[derive(Default)]
struct KeyIndices {
    /// The index of the key last set for each participant.
    latest: HashMap<ParticipantIdentity, i32>,
    /// Owned by whoever subscribed; gone ones are dropped on the next key.
    listeners: Vec<Weak<KeySetListener>>,
}

#[derive(Clone)]
pub struct KeyProvider {
    pub(crate) handle: fc::KeyProvider,
    latest_key_index: Arc<AtomicI32>,
    key_indices: Arc<Mutex<KeyIndices>>,
}

impl KeyProvider {
    /// By default, the key provider is not shared
    pub fn new(options: KeyProviderOptions) -> Self {
        Self {
            handle: fc::KeyProvider::new(fc::KeyProviderOptions {
                shared_key: false,
                ratchet_window_size: options.ratchet_window_size,
                ratchet_salt: options.ratchet_salt,
                failure_tolerance: options.failure_tolerance,
                key_ring_size: options.key_ring_size,
                key_derivation_algorithm: options.key_derivation_algorithm,
            }),
            latest_key_index: Arc::new(AtomicI32::new(0)),
            key_indices: Default::default(),
        }
    }

    pub fn with_shared_key(options: KeyProviderOptions, shared_key: Vec<u8>) -> Self {
        let handle = fc::KeyProvider::new(fc::KeyProviderOptions {
            shared_key: true,
            ratchet_window_size: options.ratchet_window_size,
            ratchet_salt: options.ratchet_salt,
            failure_tolerance: options.failure_tolerance,
            key_ring_size: options.key_ring_size,
            key_derivation_algorithm: options.key_derivation_algorithm,
        });
        handle.set_shared_key(0, shared_key);
        Self {
            handle,
            latest_key_index: Arc::new(AtomicI32::new(0)),
            key_indices: Default::default(),
        }
    }

    pub fn key_ring_size(&self) -> i32 {
        self.handle.key_ring_size()
    }

    /// Sets the shared key at `key_index`, which local tracks encrypt with from then on.
    pub fn set_shared_key(&self, shared_key: Vec<u8>, key_index: i32) -> bool {
        if self.handle.set_shared_key(key_index, shared_key) {
            self.latest_key_index.store(key_index, Ordering::Relaxed);
            self.notify_key_set(None, key_index);
            true
        } else {
            false
        }
    }

    pub fn ratchet_shared_key(&self, key_index: i32) -> Option<Vec<u8>> {
        self.handle.ratchet_shared_key(key_index)
    }

    pub fn get_shared_key(&self, key_index: i32) -> Option<Vec<u8>> {
        self.handle.get_shared_key(key_index)
    }

    /// Sets the key of a participant at `key_index`.
    ///
    /// For the local participant this is also the key its tracks encrypt with from then on,
    /// as in the other LiveKit SDKs.
    pub fn set_key(&self, identity: &ParticipantIdentity, key_index: i32, key: Vec<u8>) -> bool {
        if self.handle.set_key(identity.to_string(), key_index, key) {
            self.latest_key_index.store(key_index, Ordering::Relaxed);
            self.key_indices.lock().latest.insert(identity.clone(), key_index);
            self.notify_key_set(Some(identity), key_index);
            true
        } else {
            false
        }
    }

    pub fn ratchet_key(&self, identity: &ParticipantIdentity, key_index: i32) -> Option<Vec<u8>> {
        self.handle.ratchet_key(identity.to_string(), key_index)
    }

    pub fn get_key(&self, identity: &ParticipantIdentity, key_index: i32) -> Option<Vec<u8>> {
        self.handle.get_key(identity.to_string(), key_index)
    }

    pub fn set_sif_trailer(&self, trailer: Vec<u8>) {
        self.handle.set_sif_trailer(trailer);
    }

    pub fn get_latest_key_index(&self) -> i32 {
        self.latest_key_index.load(Ordering::Relaxed)
    }

    /// The index of the key a participant's media is to be encrypted with: that of the key
    /// last set for it, or of the last key set at all if it has none of its own.
    pub(crate) fn latest_key_index_for(&self, identity: &ParticipantIdentity) -> i32 {
        let own = self.key_indices.lock().latest.get(identity).copied();
        own.unwrap_or_else(|| self.get_latest_key_index())
    }

    /// Has `listener` called whenever a key is set, for as long as the caller keeps it alive.
    pub(crate) fn subscribe_key_set(&self, listener: &Arc<KeySetListener>) {
        self.key_indices.lock().listeners.push(Arc::downgrade(listener));
    }

    fn notify_key_set(&self, identity: Option<&ParticipantIdentity>, key_index: i32) {
        let listeners: Vec<_> = {
            let mut key_indices = self.key_indices.lock();
            key_indices.listeners.retain(|listener| listener.strong_count() > 0);
            key_indices.listeners.iter().filter_map(Weak::upgrade).collect()
        };
        // Called without the lock, so that a listener may ask for an index.
        for listener in listeners {
            listener(identity, key_index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_provider_options_defaults() {
        let options = KeyProviderOptions::default();
        assert_eq!(options.key_ring_size, DEFAULT_KEY_RING_SIZE);
        assert_eq!(options.ratchet_window_size, DEFAULT_RATCHET_WINDOW_SIZE);
        assert_eq!(MAX_KEY_RING_SIZE, 255);
    }

    #[test]
    fn latest_key_index_is_per_participant() {
        let provider = KeyProvider::new(KeyProviderOptions::default());
        let (local, remote) =
            (ParticipantIdentity::from("local"), ParticipantIdentity::from("remote"));
        assert!(provider.set_key(&local, 1, vec![1; 32]));
        assert!(provider.set_key(&remote, 3, vec![2; 32]));
        // A key of someone else does not change what we encrypt with.
        assert_eq!(provider.latest_key_index_for(&local), 1);
        assert_eq!(provider.latest_key_index_for(&remote), 3);
        // Without a key of its own a participant goes by the last key set, as before.
        assert_eq!(provider.latest_key_index_for(&ParticipantIdentity::from("other")), 3);
    }

    #[test]
    fn key_set_listeners_are_told_and_let_go() {
        let provider = KeyProvider::new(KeyProviderOptions::default());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let listener: Arc<KeySetListener> = Arc::new({
            let seen = seen.clone();
            move |identity, key_index| seen.lock().push((identity.cloned(), key_index))
        });
        provider.subscribe_key_set(&listener);

        let local = ParticipantIdentity::from("local");
        assert!(provider.set_key(&local, 2, vec![1; 32]));
        assert!(!provider.set_key(&local, -1, vec![1; 32]));
        assert_eq!(*seen.lock(), [(Some(local.clone()), 2)]);

        // The provider does not keep a listener alive, nor call one that is gone.
        let weak = Arc::downgrade(&listener);
        drop(listener);
        assert!(weak.upgrade().is_none());
        assert!(provider.set_key(&local, 3, vec![1; 32]));
        assert_eq!(seen.lock().len(), 1);
        assert!(provider.key_indices.lock().listeners.is_empty());
    }
}
