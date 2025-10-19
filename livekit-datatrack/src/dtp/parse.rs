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

use super::common::{Encryption, consts::*};
use core::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("buffer is too short to contain a valid header")]
    TooShort,

    #[error("header with length {0} exceeds buffer length")]
    HeaderOverrun(usize),
}

pub struct Dtp<'a> {
    buffer: &'a [u8],
}

/// Extract an integer of type `t` from a byte slice at a given `offset`,
/// interpreting the bytes as big-endian.
macro_rules! extract {
    ($t:ty, $bytes:expr, $offset:expr) => {{
        use std::mem::size_of;
        let slice: [u8; size_of::<$t>()] = $bytes[$offset..$offset + size_of::<$t>()]
            .try_into()
            .unwrap();
        <$t>::from_be_bytes(slice)
    }};
}

impl<'a> Dtp<'a> {
    /// Packet version.
    pub fn version(&self) -> u8 {
        (self.buffer[0] & VERSION_MASK) >> VERSION_SHIFT
    }

    /// Whether the packet's final flag is set.
    pub fn is_final(&self) -> bool {
        ((self.buffer[0] & FINAL_FLAG_MASK) >> FINAL_FLAG_SHIFT) != 0
    }

    /// Whether the packet is encrypted.
    pub fn is_encrypted(&self) -> bool {
        ((self.buffer[0] & E2EE_FLAG_MASK) >> E2EE_FLAG_SHIFT) != 0
    }

    /// Whether the packet has a timestamp extension.
    pub fn has_timestamp(&self) -> bool {
        ((self.buffer[0] & TS_FLAG_MASK) >> TS_FLAG_SHIFT) != 0
    }

    /// Whether the packet has a user timestamp extension.
    pub fn has_user_timestamp(&self) -> bool {
        ((self.buffer[0] & UTS_FLAG_MASK) >> UTS_FLAG_SHIFT) != 0
    }

    /// Total length of all header extensions expressed in number of 32-bit words.
    fn extension_words(&self) -> u8 {
        self.buffer[EXT_WORDS_OFFSET]
    }

    /// Track handle.
    pub fn track_handle(&self) -> u16 {
        extract!(u16, self.buffer, TRACK_HANDLE_OFFSET)
    }

    /// Sequence number.
    pub fn sequence(&self) -> u16 {
        extract!(u16, self.buffer, SEQUENCE_OFFSET)
    }

    /// Timestamp, if the packet has one.
    pub fn timestamp(&self) -> Option<u32> {
        self.has_timestamp()
            .then(|| extract!(u32, self.buffer, EXT_START_OFFSET))
    }

    /// User timestamp, if the packet has one.
    pub fn user_timestamp(&self) -> Option<u64> {
        if !self.has_user_timestamp() {
            None?
        }
        let offset = EXT_START_OFFSET
            + self
                .has_timestamp()
                .then_some(TIMESTAMP_EXT_LEN)
                .unwrap_or_default();
        extract!(u64, self.buffer, offset).into()
    }

    /// Encryption details.
    pub fn encryption(&self) -> Option<Encryption> {
        if !self.is_encrypted() {
            None?
        }
        let offset = EXT_START_OFFSET
            + self
                .has_timestamp()
                .then_some(TIMESTAMP_EXT_LEN)
                .unwrap_or_default()
            + self
                .has_user_timestamp()
                .then_some(USER_TIMESTAMP_EXT_LEN)
                .unwrap_or_default();
        let extension = &self.buffer[offset..(offset + E2EE_EXT_LEN)];
        let iv = extension[..E2EE_EXT_IV_LEN].try_into().unwrap();
        let key_index = extension[E2EE_EXT_KEY_INDEX_OFFSET];

        Encryption { iv, key_index }.into()
    }

    /// The payload section of the packet.
    pub fn payload(&self) -> &[u8] {
        &self.buffer[self.header_len()..]
    }

    fn header_len(&self) -> usize {
        (4 * self.extension_words() as usize) + BASE_HEADER_LEN
    }
}

impl<'a> TryFrom<&'a [u8]> for Dtp<'a> {
    type Error = ParseError;

    fn try_from(buffer: &'a [u8]) -> Result<Self, Self::Error> {
        if buffer.len() < BASE_HEADER_LEN {
            Err(ParseError::TooShort)?
        }
        let packet = Dtp { buffer };
        if packet.header_len() > buffer.len() {
            Err(ParseError::HeaderOverrun(packet.header_len()))?
        }
        Ok(packet)
    }
}

impl<'a> fmt::Debug for Dtp<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dtp")
            .field("version", &self.version())
            .field("is_final", &self.is_final())
            .field("is_encrypted", &self.is_encrypted())
            .field("track_handle", &self.track_handle())
            .field("sequence", &self.sequence())
            .field("timestamp", &self.timestamp())
            .field("user_timestamp", &self.user_timestamp())
            .field("payload_len", &self.payload().len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_buffer() {
        let buffer = [0xFF; 4];
        let dtp: Result<Dtp, ParseError> = buffer.as_ref().try_into();
        assert!(matches!(dtp, Err(ParseError::TooShort)));
    }

    #[test]
    fn test_header_overrun() {
        let buffer = [
            0x00, 0x00, 0x00, 0x01, // 1 extension word, would overrun buffer
            0x00, 0x00, 0x00, 0x00,
        ];
        let dtp: Result<Dtp, ParseError> = buffer.as_ref().try_into();
        assert!(matches!(dtp, Err(ParseError::HeaderOverrun(12))));
    }

    #[test]
    fn test_field_accessors() {
        let buffer = [
            0x1E, 0x00, // Version 0 | E2EE | Timestamp | User Timestamp
            0x00, 0x07, // Extension Words
            0x04, 0xAB, // Track Handle
            0x04, 0xD2, // Sequence
            0x01, 0x02, // Timestamp
            0x08, 0x40, // ...
            0x01, 0x00, // User Timestamp
            0x01, 0x00, // ...
            0x01, 0x00, // ...
            0x01, 0x00, // ...
            0xFA, 0xFA, // E2EE
            0xFA, 0xFA, // ...
            0xFA, 0xFA, // ...
            0xFA, 0xFA, // ...
            0xFA, 0xFA, // ...
            0xFA, 0xFA, // ...
            0x00, 0x00, // Padding
            0x00, 0x48, // Key Index
            0xFA, 0xAF, // Payload
        ];

        let dtp: Dtp = buffer.as_ref().try_into().unwrap();
        assert_eq!(dtp.version(), 0);
        assert!(dtp.is_final());
        assert!(dtp.is_encrypted());
        assert!(dtp.has_timestamp());
        assert!(dtp.has_user_timestamp());
        assert_eq!(dtp.track_handle(), 1195);
        assert_eq!(dtp.sequence(), 1234);
        assert_eq!(dtp.extension_words(), 7);
        assert_eq!(dtp.timestamp(), Some(16_910_400));
        assert_eq!(dtp.user_timestamp(), Some(72_058_693_566_333_184));

        let encryption = dtp.encryption().unwrap();
        assert_eq!(encryption.key_index(), 72);
        assert_eq!(encryption.iv(), &[0xFA; 12]);

        assert_eq!(dtp.payload(), &[0xFA, 0xAF]);
        println!("{:?}", dtp);
    }
}
