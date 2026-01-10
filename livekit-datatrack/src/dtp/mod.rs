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

mod extension;
mod deserialize;
mod handle;
mod serialize;
mod time;

pub use extension::*;
pub use deserialize::*;
pub use handle::*;
pub use serialize::*;
pub use time::*;

#[derive(Clone)]
pub struct Dtp {
    pub header: Header,
    pub payload: Bytes,
}

#[derive(Debug, Clone)]
pub struct Header {
    pub marker: FrameMarker,
    pub track_handle: Handle,
    pub sequence: u16,
    pub frame_number: u16,
    pub timestamp: Timestamp<90_000>,
    pub extensions: Extensions
}


/// Marker indicating a packet's position in relation to a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameMarker {
    /// Packet is within a frame.
    Inter,
    /// Packet is the last in a frame.
    Final,
    /// Packet is the first in a frame.
    Start,
    /// Packet is the only one in a frame.
    Single
}

impl fmt::Debug for Dtp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dtp")
            .field("header", &self.header)
            .field("payload_len", &self.payload.len())
            .finish()
    }
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
    pub const EXT_MARKER_LEN: usize = 4;
    pub const EXT_TAG_PADDING: u16 = 0;
}
