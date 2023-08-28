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

use core::fmt;

use livekit_webrtc::frame_cryptor::KeyProviderOptions;

use super::key_provider::BaseKeyProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionType {
    None,
    Gcm,
    Custom,
}

impl fmt::Display for EncryptionType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Gcm => write!(f, "gcm"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

impl From<i32> for EncryptionType {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Gcm,
            2 => Self::Custom,
            i32::MIN..=-1_i32 | 3_i32..=i32::MAX => todo!()
        }
    }
}

impl From<livekit_protocol::encryption::Type> for EncryptionType {
    fn from(value: livekit_protocol::encryption::Type) -> Self {
        match value {
            livekit_protocol::encryption::Type::None => Self::None,
            livekit_protocol::encryption::Type::Gcm => Self::Gcm,
            livekit_protocol::encryption::Type::Custom => Self::Custom,
        }
    }
}

impl From<EncryptionType> for livekit_protocol::encryption::Type {
    fn from(value: EncryptionType) -> Self {
        match value {
            EncryptionType::None => Self::None,
            EncryptionType::Gcm => Self::Gcm,
            EncryptionType::Custom => Self::Custom,
        }
    }
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
                KeyProviderOptions::default(),
                true,
                "".to_string(),
            ),
        }
    }
}
