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

//! Foundational types shared across LiveKit crates: participant identities, the
//! encryption enum, and client-protocol constants.

use std::fmt::Display;

use livekit_protocol as proto;

mod enum_dispatch;

// -------------------------------------------------------------------------------------------------
// Client protocol
// -------------------------------------------------------------------------------------------------

/// Legacy client. No v2 data-stream features.
pub const CLIENT_PROTOCOL_DEFAULT: i32 = 0;

/// RPC v2 (see RPC spec). No v2 data-stream features.
pub const CLIENT_PROTOCOL_DATA_STREAM_RPC: i32 = 1;

// -------------------------------------------------------------------------------------------------
// ParticipantIdentity
// -------------------------------------------------------------------------------------------------

#[derive(Clone, Default, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct ParticipantIdentity(pub String);

impl From<String> for ParticipantIdentity {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ParticipantIdentity {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<ParticipantIdentity> for String {
    fn from(value: ParticipantIdentity) -> Self {
        value.0
    }
}

impl Display for ParticipantIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ParticipantIdentity {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// -------------------------------------------------------------------------------------------------
// EncryptionType
// -------------------------------------------------------------------------------------------------

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionType {
    #[default]
    None,
    Gcm,
    Custom,
}

impl From<proto::encryption::Type> for EncryptionType {
    fn from(value: proto::encryption::Type) -> Self {
        match value {
            proto::encryption::Type::None => Self::None,
            proto::encryption::Type::Gcm => Self::Gcm,
            proto::encryption::Type::Custom => Self::Custom,
        }
    }
}

impl From<EncryptionType> for proto::encryption::Type {
    fn from(value: EncryptionType) -> Self {
        match value {
            EncryptionType::None => Self::None,
            EncryptionType::Gcm => Self::Gcm,
            EncryptionType::Custom => Self::Custom,
        }
    }
}

impl From<EncryptionType> for i32 {
    fn from(value: EncryptionType) -> Self {
        match value {
            EncryptionType::None => 0,
            EncryptionType::Gcm => 1,
            EncryptionType::Custom => 2,
        }
    }
}
