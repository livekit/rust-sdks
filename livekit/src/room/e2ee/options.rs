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


const DEFAULT_RATCHET_SALT: &str = "LKFrameEncryptionKey";
const DEFAULT_MAGIC_BYTES: &str = "LK-ROCKS";
const DEFAULT_RATCHET_WINDOW_SIZE: i32 = 16;

#[derive(Debug, Clone)]
pub enum EncryptionType {
    None,
    Gcm,
    Custom,
}

#[derive(Debug, Clone)]
pub struct E2EEOptions {
    pub encryption_type: EncryptionType,
    pub shared_key: String,
    pub key_provider: Option<SharedPtr<KeyProvider>>,
}

impl Default for E2EEOptions {
    fn default() -> Self {
        Self {
            encryption_type: EncryptionType::Gcm,
            shared_key: "".to_string(),
            key_provider: new_key_provider(KeyProviderOptions {
                shared_key: true,
                ratchet_window_size: DEFAULT_RATCHET_WINDOW_SIZE,
                ratchet_salt: DEFAULT_RATCHET_SALT.as_bytes().to_vec(),
                uncrypted_magic_bytes: DEFAULT_MAGIC_BYTES.as_bytes().to_vec(),
            }),
        }
    }
}