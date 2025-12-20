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

use crate::dtp::time::Timestamp;

use super::{
    packet::{consts::*, Dtp, E2ee, Header},
    track_handle::{TrackHandle, TrackHandleError},
};
use bytes::{Buf, Bytes};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DeserializeError {
    #[error("too short to contain a valid header")]
    TooShort,

    #[error("header exceeds total packet length")]
    HeaderOverrun,

    #[error("unsupported version {0}")]
    UnsupportedVersion(u8),

    #[error("invalid track handle: {0}")]
    InvalidTrackHandle(#[from] TrackHandleError),

    #[error("extension with id {0} is malformed")]
    MalformedExt(u8),

    #[error("{0} is not a valid extension id")]
    InvalidExtId(u8),
}

#[derive(Debug, Default)]
struct Extensions {
    user_timestamp: Option<u64>,
    e2ee: Option<E2ee>,
}

impl Dtp {
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
        let is_final = (initial >> FINAL_FLAG_SHIFT & FINAL_FLAG_MASK) > 0;

        let extension_words = raw.get_u8();
        let ext_len = 4 * extension_words as usize;

        let track_handle: TrackHandle = raw.get_u16().try_into()?;
        let sequence = raw.get_u16();
        let frame_number = raw.get_u16();
        let timestamp = Timestamp::from_ticks(raw.get_u32());

        if ext_len > raw.remaining() {
            Err(DeserializeError::HeaderOverrun)?
        }
        let ext_block = raw.copy_to_bytes(ext_len);
        let extensions = Extensions::parse(ext_block)?;

        let header = Header {
            is_final,
            track_handle,
            sequence,
            frame_number,
            timestamp,
            user_timestamp: extensions.user_timestamp,
            e2ee: extensions.e2ee,
        };
        Ok(header)
    }
}

impl Extensions {
    fn parse(mut raw: impl Buf) -> Result<Self, DeserializeError> {
        let mut extensions = Self::default();
        while raw.remaining() > 0 {
            let initial = raw.get_u8();
            if initial == 0 {
                // Skip padding
                continue;
            }
            let ext_id = initial >> 4;
            match ext_id {
                EXT_ID_E2EE => {
                    if raw.remaining() < EXT_LEN_E2EE {
                        Err(DeserializeError::MalformedExt(ext_id))?
                    }
                    let key_index = raw.get_u8();
                    let mut iv = [0u8; 12];
                    raw.copy_to_slice(&mut iv);
                    extensions.e2ee = E2ee { key_index, iv }.into();
                }
                EXT_ID_USER_TIMESTAMP => {
                    if raw.remaining() < EXT_LEN_USER_TIMESTAMP {
                        Err(DeserializeError::MalformedExt(ext_id))?
                    }
                    extensions.user_timestamp = raw.get_u64().into()
                }
                EXT_ID_INVALID => Err(DeserializeError::InvalidExtId(EXT_ID_INVALID))?,
                _ => {
                    // Skip over unknown extensions (forward compatible).
                    let ext_len = ((initial & (0xFF ^ 0xF0)) + 1) as usize;
                    if raw.remaining() < ext_len {
                        Err(DeserializeError::MalformedExt(ext_id))?
                    }
                    raw.advance(ext_len);
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

    /// Returns the simplest valid packet to use in test.
    fn valid_packet() -> BytesMut {
        let mut raw = BytesMut::zeroed(12); // Base header
        raw[3] = 1; // Non-zero track ID
        raw
    }

    #[test]
    fn test_short_buffer() {
        let mut raw = valid_packet();
        raw.truncate(11);

        let dtp = Dtp::deserialize(raw.freeze());
        assert!(matches!(dtp, Err(DeserializeError::TooShort)));
    }

    #[test]
    fn test_header_overrun() {
        let mut raw = valid_packet();
        raw[1] = 1; // 1 extension word, would overrun buffer

        let dtp = Dtp::deserialize(raw.freeze());
        assert!(matches!(dtp, Err(DeserializeError::HeaderOverrun)));
    }

    #[test]
    fn test_unsupported_version() {
        let mut raw = valid_packet();
        raw[0] = 0x20; // Version 1 (not supported yet)

        let dtp = Dtp::deserialize(raw.freeze());
        assert!(matches!(dtp, Err(DeserializeError::UnsupportedVersion(1))));
    }

    #[test]
    fn test_base_header() {
        let mut raw = BytesMut::new();
        raw.put_u8(0x10); // Version 0, final flag set
        raw.put_u8(0x0); // No extension words
        raw.put_slice(&[0x88, 0x11]); // Track ID
        raw.put_slice(&[0x44, 0x22]); // Sequence
        raw.put_slice(&[0x44, 0x11]); // Frame number
        raw.put_slice(&[0x44, 0x22, 0x11, 0x88]); // Timestamp

        let dtp = Dtp::deserialize(raw.freeze()).unwrap();
        assert_eq!(dtp.header.is_final, true);
        assert_eq!(dtp.header.track_handle, 0x8811u32.try_into().unwrap());
        assert_eq!(dtp.header.sequence, 0x4422);
        assert_eq!(dtp.header.frame_number, 0x4411);
        assert_eq!(dtp.header.timestamp, Timestamp::from_ticks(0x44221188));
        assert_eq!(dtp.header.user_timestamp, None);
        assert_eq!(dtp.header.e2ee, None);
    }

    #[test]
    fn test_ext_skips_padding() {
        let mut raw = valid_packet();
        raw[1] = 4; // 1 extension word
        raw.put_bytes(0x00, 32 * 4); // Padding
        Dtp::deserialize(raw.freeze()).unwrap();
    }

    #[test]
    fn test_ext_e2ee() {
        let mut raw = valid_packet();
        raw[1] = 4; // 4 extension words
        raw.put_u8(0x1C); // ID 1, length 12
        raw.put_u8(0xFA); // Key index
        raw.put_bytes(0x3C, 12); // IV
        raw.put_bytes(0x00, 2); // Padding

        let dtp = Dtp::deserialize(raw.freeze()).unwrap();
        let e2ee = dtp.header.e2ee.unwrap();
        assert_eq!(e2ee.key_index, 0xFA);
        assert_eq!(e2ee.iv, [0x3C; 12]);
    }

    #[test]
    fn test_ext_user_timestamp() {
        let mut raw = valid_packet();
        raw[1] = 3; // 3 extension words
        raw.put_u8(0x27); // ID 2, length 7
        raw.put_slice(&[0x44, 0x11, 0x22, 0x11, 0x11, 0x11, 0x88, 0x11]); // User timestamp
        raw.put_bytes(0x00, 3); // Padding
                                // TODO: decreasing to 2 is header overrun (should be padding error)
        let dtp = Dtp::deserialize(raw.freeze()).unwrap();
        assert_eq!(dtp.header.user_timestamp, Some(0x4411221111118811));
    }

    #[test]
    fn test_ext_unknown() {
        let mut raw = valid_packet();
        raw[1] = 1; // 1 extension word
        raw.put_u8(0x80); // ID 8 (unknown)
        raw.put_bytes(0x00, 3); // Padding
        Dtp::deserialize(raw.freeze()).expect("Should skip unknown extension");
    }

    #[test]
    fn test_ext_id_invalid() {
        let mut raw = valid_packet();
        raw[1] = 1; // 1 extension word
        raw.put_u8(0xF0); // ID 15, invalid
        raw.put_bytes(0x00, 3); // Padding

        let dtp = Dtp::deserialize(raw.freeze());
        assert!(matches!(dtp, Err(DeserializeError::InvalidExtId(15))));
    }

    #[test]
    fn test_ext_required_word_alignment() {
        let mut raw = valid_packet();
        raw[1] = 1; // 1 extension word
        raw.put_bytes(0x00, 3); // Padding, missing one byte

        assert!(Dtp::deserialize(raw.freeze()).is_err());
    }
}
