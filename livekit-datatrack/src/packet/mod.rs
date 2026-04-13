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

use bytes::Bytes;
use core::fmt;

mod deserialize;
mod extension;
mod handle;
mod serialize;
mod time;

pub use extension::*;
pub use handle::*;
pub use time::*;

/// Maximum serialized packet size the local pipeline targets for a single
/// transport packet on the data-track channel.
pub(crate) const TRANSPORT_MTU: usize = 16_000;

#[derive(Clone)]
pub struct Packet {
    pub header: Header,
    pub payload: Bytes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct Header {
    pub marker: FrameMarker,
    pub track_handle: Handle,
    pub sequence: u16,
    pub frame_number: u16,
    pub timestamp: Timestamp<90_000>,
    pub extensions: Extensions,
}

/// Marker indicating a packet's position in relation to a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub enum FrameMarker {
    /// Packet is the first in a frame.
    Start,
    /// Packet is within a frame.
    Inter,
    /// Packet is the last in a frame.
    Final,
    /// Packet is the only one in a frame.
    Single,
}

impl fmt::Debug for Packet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Packet")
            .field("header", &self.header)
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

pub(crate) fn header_serialized_len(uses_e2ee: bool, has_user_timestamp: bool) -> usize {
    use consts::{BASE_HEADER_LEN, EXT_WORDS_INDICATOR_SIZE};

    // Each extension serializes as [tag, len, payload...].
    let mut ext_len: usize = 0;
    if uses_e2ee {
        ext_len += 2 + 13;
    }
    if has_user_timestamp {
        ext_len += 2 + 8;
    }
    if ext_len == 0 {
        return BASE_HEADER_LEN;
    }

    let ext_words = ext_len.div_ceil(4);
    BASE_HEADER_LEN + EXT_WORDS_INDICATOR_SIZE + (ext_words * 4)
}

/// Constants used for serialization and deserialization.
pub(crate) mod consts {
    pub const SUPPORTED_VERSION: u8 = 0;
    pub const BASE_HEADER_LEN: usize = 12;

    // Bitfield shifts and masks for header flags
    pub const VERSION_SHIFT: u8 = 5;
    pub const VERSION_MASK: u8 = 0x07;

    pub const FRAME_MARKER_SHIFT: u8 = 3;
    pub const FRAME_MARKER_MASK: u8 = 0x3;

    pub const FRAME_MARKER_START: u8 = 0x2;
    pub const FRAME_MARKER_FINAL: u8 = 0x1;
    pub const FRAME_MARKER_INTER: u8 = 0x0;
    pub const FRAME_MARKER_SINGLE: u8 = 0x3;

    pub const EXT_WORDS_INDICATOR_SIZE: usize = 2;
    pub const EXT_FLAG_SHIFT: u8 = 0x2;
    pub const EXT_FLAG_MASK: u8 = 0x1;
    pub const EXT_MARKER_LEN: usize = 2;
    pub const EXT_TAG_PADDING: u8 = 0;
}

#[cfg(test)]
mod tests {
    use super::Packet;
    use fake::{Fake, Faker};

    #[test]
    fn test_roundtrip() {
        let original: Packet = Faker.fake();

        let header = original.header.clone();
        let payload = original.payload.clone();

        let serialized = original.serialize();
        let deserialized = Packet::deserialize(serialized).unwrap();

        assert_eq!(deserialized.header, header);
        assert_eq!(deserialized.payload, payload);
    }
}
