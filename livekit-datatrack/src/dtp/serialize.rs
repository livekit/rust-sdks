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

use super::packet::{Dtp, Header, consts::*};
use bytes::{BufMut, Bytes, BytesMut};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SerializeError {
    #[error("buffer cannot fit header")]
    TooSmallForHeader,

    #[error("buffer cannot fit payload")]
    TooSmallForPayload,
}

impl Dtp {
    /// Serialize the packet into a new buffer.
    pub fn serialize(self) -> Bytes {
        let len = self.serialized_len();
        let mut buf = BytesMut::with_capacity(len);

        let written = self.serialize_into(&mut buf).unwrap();
        assert_eq!(written, len);
        buf.freeze()
    }

    /// Serialize the packet into the given buffer.
    ///
    /// If the given buffer is too short to accommodate the serialized packet, the result
    /// is an error. Use [`Self::serialized_len()`] to get the required buffer size.
    ///
    pub fn serialize_into(self, buf: &mut impl BufMut) -> Result<usize, SerializeError> {
        let payload_len = self.payload.len();
        let header_len = self.header.serialize_into(buf)?;
        if buf.remaining_mut() < self.payload.len() {
            Err(SerializeError::TooSmallForPayload)?
        }
        buf.put(self.payload);
        Ok(header_len + payload_len)
    }
}

impl Dtp {
    /// Length of the serialized packet in bytes.
    pub fn serialized_len(&self) -> usize {
        self.header.serialized_len() + self.payload.len()
    }
}

impl Header {
    fn serialize_into(self, buf: &mut impl BufMut) -> Result<usize, SerializeError> {
        let metrics = self.metrics();
        if buf.remaining_mut() < metrics.len {
            Err(SerializeError::TooSmallForHeader)?
        }

        let mut initial = SUPPORTED_VERSION << VERSION_SHIFT;
        if self.is_final {
            initial |= 1 << FINAL_FLAG_SHIFT;
        }
        buf.put_u8(initial);
        buf.put_u8(metrics.ext_words as u8);

        buf.put_u16(self.track_handle.into());
        buf.put_u16(self.sequence);
        buf.put_u16(self.frame_number);
        buf.put_u32(self.timestamp);

        if let Some(e2ee) = &self.e2ee {
            buf.put_u8(EXT_MARKER_E2EE);
            buf.put_u8(e2ee.key_index);
            buf.put_slice(&e2ee.iv);
        }
        if let Some(user_timestamp) = self.user_timestamp {
            buf.put_u8(EXT_MARKER_USER_TIMESTAMP);
            buf.put_u64(user_timestamp);
        }
        buf.put_bytes(0, metrics.padding_len);

        Ok(metrics.len)
    }
}

#[derive(Debug)]
struct HeaderMetrics {
    /// Number of 32-bit extension words.
    ext_words: usize,
    /// Number of padding bytes needed to align extension block.
    padding_len: usize,
    /// Total size of the serialized header in bytes
    len: usize,
}

impl Header {
    /// Length of the serialized header in bytes.
    fn serialized_len(&self) -> usize {
        self.metrics().len
    }

    /// Length of all extensions not including padding.
    fn ext_len(&self) -> usize {
        let mut len = 0;
        if self.e2ee.is_some() {
            len += EXT_MARKER_LEN + EXT_LEN_E2EE;
        }
        if self.user_timestamp.is_some() {
            len += EXT_MARKER_LEN + EXT_LEN_USER_TIMESTAMP;
        }
        len
    }

    /// Header metrics required for buffer sizing and serialization.
    fn metrics(&self) -> HeaderMetrics {
        let ext_len = self.ext_len();
        let ext_words = ext_len.div_ceil(4);
        assert!(ext_words <= u8::MAX.into());
        let padding_len = (ext_words as usize * 4) - ext_len;
        let len = BASE_HEADER_LEN + ext_len + padding_len;
        HeaderMetrics {
            ext_words,
            padding_len,
            len,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{*, super::packet::E2ee};
    use bytes::Buf;

    /// Constructed packet to use in tests.
    fn packet() -> Dtp {
        Dtp {
            header: Header {
                version: 0,
                is_final: true,
                track_handle: 0x8811u32.try_into().unwrap(),
                sequence: 0x4422,
                frame_number: 0x4411,
                timestamp: 0x44221188,
                user_timestamp: 0x4411221111118811.into(),
                e2ee: E2ee {
                    key_index: 0xFA,
                    iv: [0x3C; 12],
                }
                .into(),
            },
            payload: vec![0xFA; 1024].into(),
        }
    }

    #[test]
    fn test_header_metrics() {
        let metrics = packet().header.metrics();
        assert_eq!(metrics.ext_words, 6);
        assert_eq!(metrics.padding_len, 1);
        assert_eq!(metrics.len, 36);
    }

    #[test]
    fn test_serialize() {
        let mut buf = packet().serialize().try_into_mut().unwrap();
        assert_eq!(buf.len(), 1060);

        // Base header
        assert_eq!(buf.get_u8(), 0x10); // Version 0, final flag set
        assert_eq!(buf.get_u8(), 6); // Extension words
        assert_eq!(buf.get_u16(), 0x8811); // Track handle
        assert_eq!(buf.get_u16(), 0x4422); // Sequence
        assert_eq!(buf.get_u16(), 0x4411); // Frame number
        assert_eq!(buf.get_u32(), 0x44221188); // Timestamp

        // E2EE extension
        assert_eq!(buf.get_u8(), 0x1C); // ID 1, length 12
        assert_eq!(buf.get_u8(), 0xFA); // Key index
        assert_eq!(buf.copy_to_bytes(12), vec![0x3C; 12]);

        // User timestamp extension
        assert_eq!(buf.get_u8(), 0x27); // ID 2, length 7
        assert_eq!(buf.get_u64(), 0x4411221111118811);

        assert_eq!(buf.get_u8(), 0); // Padding
        assert_eq!(buf.copy_to_bytes(1024), vec![0xFA; 1024]); // Payload

        assert_eq!(buf.remaining(), 0);
    }
}
