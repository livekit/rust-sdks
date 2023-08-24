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

use livekit_webrtc::frame_cryptor::KeyProviderOptions;

use super::key_provider::BaseKeyProvider;

const DEFAULT_RATCHET_SALT: &str = "LKFrameEncryptionKey";
const DEFAULT_MAGIC_BYTES: &str = "LK-ROCKS";
const DEFAULT_RATCHET_WINDOW_SIZE: i32 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncryptionType {
    None,
    Gcm,
    Custom,
}

#[derive(Clone)]
pub struct E2EEOptions {
    pub encryption_type: EncryptionType,
    pub key_provider: BaseKeyProvider,
}

impl E2EEOptions {
    pub fn new(encryption_type: EncryptionType, key_provider: BaseKeyProvider) -> Self {
        Self {
            encryption_type,
            key_provider,
        }
    }

    pub fn is_shared_key(&self) -> bool {
        self.key_provider.is_shared_key()
    }
}

impl Default for E2EEOptions {
    fn default() -> Self {
        Self {
            encryption_type: EncryptionType::Gcm,
            key_provider: BaseKeyProvider::new(
                KeyProviderOptions {
                    shared_key: true,
                    ratchet_window_size: DEFAULT_RATCHET_WINDOW_SIZE,
                    ratchet_salt: DEFAULT_RATCHET_SALT.as_bytes().to_vec(),
                    uncrypted_magic_bytes: DEFAULT_MAGIC_BYTES.as_bytes().to_vec(),
                },
                true,
                "12345678".to_string(),
            ),
        }
    }
}
