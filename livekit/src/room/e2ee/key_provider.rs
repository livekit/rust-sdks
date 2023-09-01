// Copyright 2023 LiveKit, Inc.
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

use livekit_webrtc::frame_cryptor as fc;

const DEFAULT_RATCHET_SALT: &str = "LKFrameEncryptionKey";
const DEFAULT_MAGIC_BYTES: &str = "LK-ROCKS";
const DEFAULT_RATCHET_WINDOW_SIZE: i32 = 16;

#[derive(Clone)]
pub struct KeyProviderOptions {
    pub shared_key: bool,
    pub ratchet_window_size: i32,
    pub ratchet_salt: Vec<u8>,
    pub uncrypted_magic_bytes: Vec<u8>,
}

impl KeyProviderOptions {
    pub fn new(
        shared_key: bool,
        ratchet_window_size: i32,
        ratchet_salt: Vec<u8>,
        uncrypted_magic_bytes: Vec<u8>,
    ) -> Self {
        Self {
            shared_key,
            ratchet_window_size,
            ratchet_salt,
            uncrypted_magic_bytes,
        }
    }
}

impl Default for KeyProviderOptions {
    fn default() -> Self {
        Self {
            shared_key: true,
            ratchet_window_size: DEFAULT_RATCHET_WINDOW_SIZE,
            ratchet_salt: DEFAULT_RATCHET_SALT.as_bytes().to_vec(),
            uncrypted_magic_bytes: DEFAULT_MAGIC_BYTES.as_bytes().to_vec(),
        }
    }
}

impl From<KeyProviderOptions> for fc::KeyProviderOptions {
    fn from(options: KeyProviderOptions) -> Self {
        Self {
            shared_key: options.shared_key,
            ratchet_window_size: options.ratchet_window_size,
            ratchet_salt: options.ratchet_salt,
            uncrypted_magic_bytes: options.uncrypted_magic_bytes,
        }
    }
}

#[derive(Clone)]
pub struct BaseKeyProvider {
    pub(crate) handle: fc::KeyProvider,
}

impl Default for BaseKeyProvider {
    fn default() -> Self {
        Self {
            handle: fc::KeyProvider::new(KeyProviderOptions::default().into()),
        }
    }
}

impl BaseKeyProvider {
    pub fn new(options: KeyProviderOptions) -> Self {
        Self {
            handle: fc::KeyProvider::new(options.into()),
        }
    }

    // create a new key provider with a shared key
    pub fn new_with_shared_key(shared_key: Vec<u8>) -> Self {
        let handle = fc::KeyProvider::new(KeyProviderOptions::default().into());
        handle.set_shared_key(0, shared_key);
        Self { handle }
    }

    // set shared key, default key index is 0
    pub fn set_shared_key(&self, shared_key: Vec<u8>, key_index: Option<i32>) {
        self.handle
            .set_shared_key(key_index.unwrap_or(0), shared_key);
    }

    // ratchet shared key by key index.
    pub fn ratchet_shared_key(&self, key_index: i32) -> Vec<u8> {
        self.handle.ratchet_shared_key(key_index)
    }

    // export shared key by key index.
    pub fn export_shared_key(&self, key_index: i32) -> Vec<u8> {
        self.handle.export_shared_key(key_index)
    }

    // set key for a participant, with a key index
    pub fn set_key(&self, participant_id: String, key_index: i32, key: Vec<u8>) -> bool {
        self.handle.set_key(participant_id, key_index, key)
    }

    // ratchet key for a participant, with a key index
    pub fn ratchet_key(&self, participant_id: String, key_index: i32) -> Vec<u8> {
        self.handle.ratchet_key(participant_id, key_index)
    }

    // export current key for a participant, with a key index
    pub fn export_key(&self, participant_id: String, key_index: i32) -> Vec<u8> {
        self.handle.export_key(participant_id, key_index)
    }
}
