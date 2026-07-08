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
//! encryption/capability enums, client-protocol constants, and the remote-participant
//! registry trait consulted by the data-stream and RPC send paths.

use std::fmt::Display;

use livekit_protocol as proto;

mod enum_dispatch;

// -------------------------------------------------------------------------------------------------
// Client protocol
// -------------------------------------------------------------------------------------------------

/// Legacy client.
pub const CLIENT_PROTOCOL_DEFAULT: i32 = 0;

/// RPC v2 (see RPC spec).
pub const CLIENT_PROTOCOL_DATA_STREAM_RPC: i32 = 1;

/// Understands inline single-packet data streams (data streams v2).
pub const CLIENT_PROTOCOL_DATA_STREAM_V2: i32 = 2;

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

// -------------------------------------------------------------------------------------------------
// ClientCapability
// -------------------------------------------------------------------------------------------------

/// A capability a participant's client advertises, mirroring the `ClientInfo.Capability` protobuf
/// enum.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub enum ClientCapability {
    Unused,
    PacketTrailer,
    CompressionDeflateRaw,
}

impl TryFrom<i32> for ClientCapability {
    type Error = &'static str;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match proto::client_info::Capability::try_from(value) {
            Ok(proto::client_info::Capability::CapPacketTrailer) => Ok(Self::PacketTrailer),
            Ok(proto::client_info::Capability::CapCompressionDeflateRaw) => {
                Ok(Self::CompressionDeflateRaw)
            }
            Ok(proto::client_info::Capability::CapUnused) => Ok(Self::Unused),
            Err(_) => Err("unknown client capability"),
        }
    }
}

impl From<ClientCapability> for i32 {
    fn from(value: ClientCapability) -> Self {
        match value {
            ClientCapability::Unused => proto::client_info::Capability::CapUnused as i32,
            ClientCapability::PacketTrailer => {
                proto::client_info::Capability::CapPacketTrailer as i32
            }
            ClientCapability::CompressionDeflateRaw => {
                proto::client_info::Capability::CapCompressionDeflateRaw as i32
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// RemoteParticipantRegistry
// -------------------------------------------------------------------------------------------------

/// Read access to remote participants' advertised protocol and capabilities.
///
/// Used by downstream modules like the the RPC transport (v1/v2 transport selection) and
/// the data-stream send path (inline / compression eligibility) to determine what level of support
/// a participant has for protocol level features.
pub trait RemoteParticipantRegistry: Send + Sync {
    /// A remote participant's `client_protocol`, or `CLIENT_PROTOCOL_DEFAULT` (0) if unknown.
    fn remote_client_protocol(&self, identity: &ParticipantIdentity) -> i32;

    /// A remote participant's advertised capabilities, or empty if unknown.
    fn remote_capabilities(&self, identity: &ParticipantIdentity) -> Vec<ClientCapability>;

    /// The identities of every remote participant, used to resolve a broadcast send.
    fn remote_identities(&self) -> Vec<ParticipantIdentity>;
}
