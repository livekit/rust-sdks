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

use super::{Handle, Timestamp};
use bytes::Bytes;
use core::fmt;

#[derive(Clone)]
pub struct Dtp {
    pub header: Header,
    pub payload: Bytes,
}

#[derive(Debug, Clone)]
pub struct Header {
    pub is_final: bool,
    pub track_handle: Handle,
    pub sequence: u16,
    pub frame_number: u16,
    pub timestamp: Timestamp<90_000>,
    pub user_timestamp: Option<u64>,
    pub e2ee: Option<E2ee>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct E2ee {
    pub key_index: u8,
    pub iv: [u8; 12],
}

impl Dtp {
    /// Whether the packet is the final one in a frame.
    pub fn is_final(&self) -> bool {
        self.header.is_final
    }

    /// Whether the packet's payload is encrypted.
    pub fn is_encrypted(&self) -> bool {
        self.header.e2ee.is_some()
    }
}

impl fmt::Debug for Dtp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dtp")
            .field("header", &self.header)
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

impl fmt::Debug for E2ee {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // For security, do not include fields in debug.
        f.debug_struct("E2ee").finish()
    }
}

/// Constants used in sterilization and deserialization.
pub(crate) mod consts {
    pub const SUPPORTED_VERSION: u8 = 0;
    pub const BASE_HEADER_LEN: usize = 12;

    // Bitfield shifts and masks for header flags
    pub const VERSION_SHIFT: u8 = 5;
    pub const VERSION_MASK: u8 = 0x07;
    pub const FINAL_FLAG_SHIFT: u8 = 4;
    pub const FINAL_FLAG_MASK: u8 = 0x01;

    // Extension IDs
    pub const EXT_ID_E2EE: u8 = 0x1;
    pub const EXT_ID_USER_TIMESTAMP: u8 = 0x2;
    pub const EXT_ID_INVALID: u8 = 0xF;

    // Extension lengths
    pub const EXT_LEN_E2EE: usize = 13;
    pub const EXT_LEN_USER_TIMESTAMP: usize = 8;

    // Extension markers
    pub const EXT_MARKER_LEN: usize = 1;
    pub const EXT_MARKER_E2EE: u8 = ext_marker(EXT_ID_E2EE, EXT_LEN_E2EE as u8);
    pub const EXT_MARKER_USER_TIMESTAMP: u8 =
        ext_marker(EXT_ID_USER_TIMESTAMP, EXT_LEN_USER_TIMESTAMP as u8);

    const fn ext_marker(id: u8, len: u8) -> u8 {
        (id << 4) | (len - 1)
    }
}
