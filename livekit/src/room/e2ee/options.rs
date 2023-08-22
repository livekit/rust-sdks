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
use crate::{
    e2ee::{
        frame_cryptor::{FrameCryptor, FrameCryptorOptions},
        key_provider::{new_key_provider, KeyProvider, KeyProviderOptions},
    },
    error::Error,
};

const default_ratchet_salt: &str = "LKFrameEncryptionKey";
const default_magic_bytes: &str = "LK-ROCKS";
const default_ratchet_window_size: i32 = 16;

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
            encryption_type: Gcm,
            shared_key: "".to_string(),
            key_provider: new_key_provider(KeyProviderOptions {
                shared_key: true,
                ratchet_window_size: default_ratchet_window_size,
                ratchet_salt: default_ratchet_salt.as_bytes().to_vec(),
                uncrypted_magic_bytes: default_magic_bytes.as_bytes().to_vec(),
            }),
        }
    }
}