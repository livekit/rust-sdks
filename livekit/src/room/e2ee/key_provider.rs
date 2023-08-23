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

use livekit_webrtc::frame_cryptor::{KeyProvider, KeyProviderOptions};

#[derive(Clone)]
pub struct BaseKeyProvider {
    is_shared_key: bool,
    shared_key: String,
    pub(crate) handle: KeyProvider,
}

impl BaseKeyProvider {
    pub fn new(options: KeyProviderOptions, is_shared_key: bool, shared_key: String) -> Self {
        Self {
            is_shared_key,
            shared_key,
            handle: KeyProvider::new(options),
        }
    }

    pub fn set_shared_key(&mut self, shared_key: String) {
        self.shared_key = shared_key;
    }

    pub fn enabled_shared_key(&mut self, enabled: bool) {
        self.is_shared_key = enabled;
    }

    pub fn is_shared_key(&self) -> bool {
        self.is_shared_key
    }

    pub fn shared_key(&self) -> String {
        self.shared_key.clone()
    }

    pub fn set_key(&self, participant_id: String, key_index: i32, key: Vec<u8>) -> bool {
        self.handle.set_key(participant_id, key_index, key)
    }

    pub fn ratchet_key(&self, participant_id: String, key_index: i32) -> Vec<u8> {
        self.handle.ratchet_key(participant_id, key_index)
    }

    pub fn export_key(&self, participant_id: String, key_index: i32) -> Vec<u8> {
        self.handle.export_key(participant_id, key_index)
    }
}
