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

//! Types that cross the FFI boundary for the data stream managers only: client capabilities, the
//! packet delivery error, and the wire-packet decode helper.

use livekit_common as common;
use livekit_data_stream::backend as ds;
use livekit_protocol as proto;
use prost::Message;

use crate::data_stream::common::DataStreamError;

// MARK: - Enums

/// A capability a participant's client advertises, mirroring [`common::ClientCapability`].
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientCapability {
    Unused,
    PacketTrailer,
    CompressionDeflateRaw,
}

impl From<common::ClientCapability> for ClientCapability {
    fn from(value: common::ClientCapability) -> Self {
        match value {
            common::ClientCapability::Unused => Self::Unused,
            common::ClientCapability::PacketTrailer => Self::PacketTrailer,
            common::ClientCapability::CompressionDeflateRaw => Self::CompressionDeflateRaw,
            // `common::ClientCapability` is `#[non_exhaustive]`; treat anything newer as unusable.
            _ => Self::Unused,
        }
    }
}

impl From<ClientCapability> for common::ClientCapability {
    fn from(value: ClientCapability) -> Self {
        match value {
            ClientCapability::Unused => Self::Unused,
            ClientCapability::PacketTrailer => Self::PacketTrailer,
            ClientCapability::CompressionDeflateRaw => Self::CompressionDeflateRaw,
        }
    }
}

// MARK: - Error

/// A foreign transport failed to deliver outbound packets; thrown by hosts from
/// [`OutgoingDataStreamManagerDelegate::on_packets_available`](super::outgoing::OutgoingDataStreamManagerDelegate::on_packets_available).
///
/// Morally `struct PacketDeliveryError(String)`, but uniffi error types must be enums, so the
/// string travels as the single variant's `reason` (free-form host context: logged, not parsed).
#[derive(uniffi::Error, thiserror::Error, Debug)]
pub enum PacketDeliveryError {
    #[error("failed to deliver packets: {reason}")]
    Failed { reason: String },
}

// Required because foreign code implements delegate methods returning this error: an exception
// that is NOT a `PacketDeliveryError` surfaces through this catch-all rather than aborting.
impl From<uniffi::UnexpectedUniFFICallbackError> for PacketDeliveryError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed { reason: error.reason }
    }
}

impl From<PacketDeliveryError> for DataStreamError {
    fn from(error: PacketDeliveryError) -> Self {
        log::error!("outbound packet delivery failed: {error}");
        Self::Internal
    }
}

// MARK: - Wire decode

/// Decodes a serialized [`proto::DataPacket`] carrying a data-stream header/chunk/trailer into an
/// incoming-manager input event. Returns `None` if the bytes don't decode or the packet isn't a
/// data-stream packet.
///
/// The encryption type must be supplied by the caller and CANNOT be recovered from the bytes:
/// `encrypted_packet` is a member of the `DataPacket.value` oneof, so a host decrypting E2EE
/// traffic replaces it with the decrypted stream header/chunk/trailer — by the time these bytes
/// arrive, the field is absent from the wire format. Hosts without E2EE pass
/// [`common::EncryptionType::None`].
pub(crate) fn decode_data_packet(
    bytes: &[u8],
    encryption_type: common::EncryptionType,
) -> Option<ds::incoming::PacketReceived> {
    let mut packet = proto::DataPacket::decode(bytes).ok()?;
    let identity: common::ParticipantIdentity = packet.participant_identity.clone().into();
    let ds_packet = match packet.value.take()? {
        proto::data_packet::Value::StreamHeader(header) => {
            ds::Packet::Header { header: header.into(), encryption_type }
        }
        proto::data_packet::Value::StreamChunk(chunk) => {
            ds::Packet::Chunk { chunk: chunk.into(), encryption_type }
        }
        proto::data_packet::Value::StreamTrailer(trailer) => {
            ds::Packet::Trailer { trailer: trailer.into(), encryption_type }
        }
        _ => return None,
    };
    Some(ds::incoming::PacketReceived::new(ds_packet, identity))
}
