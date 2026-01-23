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

use super::{
    consts::*, E2eeExt, ExtensionTag, Extensions, FrameMarker, Handle, HandleError, Header, Packet,
    Timestamp, UserTimestampExt,
};
use bytes::{Buf, Bytes};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DeserializeError {
    #[error("too short to contain a valid header")]
    TooShort,

    #[error("header exceeds total packet length")]
    HeaderOverrun,

    #[error("extension word indicator is missing")]
    MissingExtWords,

    #[error("unsupported version {0}")]
    UnsupportedVersion(u8),

    #[error("invalid track handle: {0}")]
    InvalidHandle(#[from] HandleError),

    #[error("extension with tag {0} is malformed")]
    MalformedExt(ExtensionTag),
}

impl Packet {
    pub fn deserialize(mut raw: Bytes) -> Result<Self, DeserializeError> {
        let header = Header::deserialize(&mut raw)?;
        let payload_len = raw.remaining();
        let payload = raw.copy_to_bytes(payload_len);
        Ok(Self { header, payload })
    }
}

impl Header {
    fn deserialize(raw: &mut impl Buf) -> Result<Self, DeserializeError> {
        if raw.remaining() < BASE_HEADER_LEN {
            Err(DeserializeError::TooShort)?
        }
        let initial = raw.get_u8();

        let version = initial >> VERSION_SHIFT & VERSION_MASK;
        if version > SUPPORTED_VERSION {
            Err(DeserializeError::UnsupportedVersion(version))?
        }
        let marker = match initial >> FRAME_MARKER_SHIFT & FRAME_MARKER_MASK {
            FRAME_MARKER_START => FrameMarker::Start,
            FRAME_MARKER_FINAL => FrameMarker::Final,
            FRAME_MARKER_SINGLE => FrameMarker::Single,
            _ => FrameMarker::Inter,
        };
        let ext_flag = (initial >> EXT_FLAG_SHIFT & EXT_FLAG_MASK) > 0;
        raw.advance(1); // Reserved

        let track_handle: Handle = raw.get_u16().try_into()?;
        let sequence = raw.get_u16();
        let frame_number = raw.get_u16();
        let timestamp = Timestamp::from_ticks(raw.get_u32());

        let mut extensions = Extensions::default();
        if ext_flag {
            if raw.remaining() < 2 {
                Err(DeserializeError::MissingExtWords)?;
            }
            let ext_words = raw.get_u16();

            let ext_len = 4 * (ext_words + 1) as usize;
            if ext_len > raw.remaining() {
                Err(DeserializeError::HeaderOverrun)?
            }
            let ext_block = raw.copy_to_bytes(ext_len);
            extensions = Extensions::deserialize(ext_block)?;
        }

        let header = Header { marker, track_handle, sequence, frame_number, timestamp, extensions };
        Ok(header)
    }
}

impl Extensions {
    fn deserialize(mut raw: impl Buf) -> Result<Self, DeserializeError> {
        let mut extensions = Self::default();
        while raw.remaining() >= 4 {
            let tag = raw.get_u16();
            let len = raw.get_u16() as usize;
            if tag == EXT_TAG_PADDING {
                // Skip padding
                continue;
            }
            match tag {
                E2eeExt::TAG => {
                    if raw.remaining() < E2eeExt::LEN {
                        Err(DeserializeError::MalformedExt(tag))?
                    }
                    let key_index = raw.get_u8();
                    let mut iv = [0u8; 12];
                    raw.copy_to_slice(&mut iv);
                    extensions.e2ee = E2eeExt { key_index, iv }.into();
                }
                UserTimestampExt::TAG => {
                    if raw.remaining() < UserTimestampExt::LEN {
                        Err(DeserializeError::MalformedExt(tag))?
                    }
                    extensions.user_timestamp = UserTimestampExt(raw.get_u64()).into()
                }
                _ => {
                    // Skip over unknown extensions (forward compatible).
                    if raw.remaining() < len {
                        Err(DeserializeError::MalformedExt(tag))?
                    }
                    raw.advance(len);
                    continue;
                }
            }
        }
        Ok(extensions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::{BufMut, BytesMut};
    use test_case::test_matrix;

    /// Returns the simplest valid packet to use in test.
    fn valid_packet() -> BytesMut {
        let mut raw = BytesMut::zeroed(12); // Base header
        raw[3] = 1; // Non-zero track handle
        raw
    }

    #[test]
    fn test_short_buffer() {
        let mut raw = valid_packet();
        raw.truncate(11);

        let packet = Packet::deserialize(raw.freeze());
        assert!(matches!(packet, Err(DeserializeError::TooShort)));
    }

    #[test]
    fn test_missing_ext_words() {
        let mut raw = valid_packet();
        raw[0] |= 1 << EXT_FLAG_SHIFT; // Extension flag
                                       // Should have ext word indicator here

        let packet = Packet::deserialize(raw.freeze());
        assert!(matches!(packet, Err(DeserializeError::MissingExtWords)));
    }

    #[test]
    fn test_header_overrun() {
        let mut raw = valid_packet();
        raw[0] |= 1 << EXT_FLAG_SHIFT; // Extension flag
        raw.put_u16(1); // One extension word

        let packet = Packet::deserialize(raw.freeze());
        assert!(matches!(packet, Err(DeserializeError::HeaderOverrun)));
    }

    #[test]
    fn test_unsupported_version() {
        let mut raw = valid_packet();
        raw[0] = 0x20; // Version 1 (not supported yet)

        let packet = Packet::deserialize(raw.freeze());
        assert!(matches!(packet, Err(DeserializeError::UnsupportedVersion(1))));
    }

    #[test]
    fn test_base_header() {
        let mut raw = BytesMut::new();
        raw.put_u8(0x8); // Version 0, final flag set, no extensions
        raw.put_u8(0x0); // Reserved
        raw.put_slice(&[0x88, 0x11]); // Track ID
        raw.put_slice(&[0x44, 0x22]); // Sequence
        raw.put_slice(&[0x44, 0x11]); // Frame number
        raw.put_slice(&[0x44, 0x22, 0x11, 0x88]); // Timestamp

        let packet = Packet::deserialize(raw.freeze()).unwrap();
        assert_eq!(packet.header.marker, FrameMarker::Final);
        assert_eq!(packet.header.track_handle, 0x8811u32.try_into().unwrap());
        assert_eq!(packet.header.sequence, 0x4422);
        assert_eq!(packet.header.frame_number, 0x4411);
        assert_eq!(packet.header.timestamp, Timestamp::from_ticks(0x44221188));
        assert_eq!(packet.header.extensions.user_timestamp, None);
        assert_eq!(packet.header.extensions.e2ee, None);
    }

    #[test_matrix([0, 1, 24])]
    fn test_ext_skips_padding(ext_words: usize) {
        let mut raw = valid_packet();
        raw[0] |= 1 << EXT_FLAG_SHIFT; // Extension flag

        raw.put_u16(ext_words as u16); // Extension word
        raw.put_bytes(0, (ext_words + 1) * 4); // Padding

        let packet = Packet::deserialize(raw.freeze()).unwrap();
        assert_eq!(packet.payload.len(), 0);
    }

    #[test]
    fn test_ext_e2ee() {
        let mut raw = valid_packet();
        raw[0] |= 1 << EXT_FLAG_SHIFT; // Extension flag
        raw.put_u16(4); // Extension words

        raw.put_u16(1); // ID 1
        raw.put_u16(12); // Length 12
        raw.put_u8(0xFA); // Key index
        raw.put_bytes(0x3C, 12); // IV
        raw.put_bytes(0, 3); // Padding

        let packet = Packet::deserialize(raw.freeze()).unwrap();
        let e2ee = packet.header.extensions.e2ee.unwrap();
        assert_eq!(e2ee.key_index, 0xFA);
        assert_eq!(e2ee.iv, [0x3C; 12]);
    }

    #[test]
    fn test_ext_user_timestamp() {
        let mut raw = valid_packet();
        raw[0] |= 1 << EXT_FLAG_SHIFT; // Extension flag
        raw.put_u16(2); // Extension words

        raw.put_u16(2);
        raw.put_u16(7);
        raw.put_slice(&[0x44, 0x11, 0x22, 0x11, 0x11, 0x11, 0x88, 0x11]); // User timestamp

        let packet = Packet::deserialize(raw.freeze()).unwrap();
        assert_eq!(
            packet.header.extensions.user_timestamp,
            UserTimestampExt(0x4411221111118811).into()
        );
    }

    #[test]
    fn test_ext_unknown() {
        let mut raw = valid_packet();
        raw[0] |= 1 << EXT_FLAG_SHIFT; // Extension flag
        raw.put_u16(1); // Extension words

        raw.put_u16(8); // ID 8 (unknown)
        raw.put_bytes(0, 6);
        Packet::deserialize(raw.freeze()).expect("Should skip unknown extension");
    }

    #[test]
    fn test_ext_required_word_alignment() {
        let mut raw = valid_packet();
        raw[0] |= 1 << EXT_FLAG_SHIFT; // Extension flag
        raw.put_u16(0); // Extension words
        raw.put_bytes(0, 3); // Padding, missing one byte

        assert!(Packet::deserialize(raw.freeze()).is_err());
    }
}
