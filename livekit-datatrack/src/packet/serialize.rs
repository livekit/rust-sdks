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

use super::{consts::*, E2eeExt, Extensions, FrameMarker, Header, Packet, UserTimestampExt};
use bytes::{BufMut, Bytes, BytesMut};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SerializeError {
    #[error("buffer cannot fit header")]
    TooSmallForHeader,

    #[error("buffer cannot fit payload")]
    TooSmallForPayload,
}

impl Packet {
    /// Length of the serialized packet in bytes.
    pub fn serialized_len(&self) -> usize {
        self.header.serialized_len() + self.payload.len()
    }

    /// Serialize the packet into the given buffer.
    ///
    /// If the given buffer is too short to accommodate the serialized packet, the result
    /// is an error. Use [`Self::serialized_len()`] to get the required buffer size.
    ///
    pub fn serialize_into(self, buf: &mut impl BufMut) -> Result<usize, SerializeError> {
        let payload_len = self.payload.len();
        let header_len = self.header.serialize_into(buf)?;
        if buf.remaining_mut() < payload_len {
            Err(SerializeError::TooSmallForPayload)?
        }
        buf.put(self.payload);
        Ok(header_len + payload_len)
    }

    /// Serialize the packet into a new buffer.
    pub fn serialize(self) -> Bytes {
        let len = self.serialized_len();
        let mut buf = BytesMut::with_capacity(len);

        let written = self.serialize_into(&mut buf).unwrap();
        assert_eq!(written, len);
        buf.freeze()
    }
}

struct HeaderMetrics {
    ext_len: usize,
    ext_words: usize,
    padding_len: usize,
}

impl HeaderMetrics {
    fn serialized_len(&self) -> usize {
        let mut len = BASE_HEADER_LEN;
        if self.ext_len > 0 {
            len += EXT_WORDS_INDICATOR_SIZE + self.ext_len + self.padding_len;
        }
        len
    }
}

impl Header {
    /// Lengths of individual elements in the serialized header.
    fn metrics(&self) -> HeaderMetrics {
        let ext_len = self.extensions.serialized_len();
        let ext_words = ext_len.div_ceil(4);
        let padding_len = (ext_words * 4) - ext_len;
        HeaderMetrics { ext_len, ext_words, padding_len }
    }

    /// Length of the serialized header in bytes.
    pub fn serialized_len(&self) -> usize {
        self.metrics().serialized_len()
    }

    fn serialize_into(self, buf: &mut impl BufMut) -> Result<usize, SerializeError> {
        let metrics = self.metrics();
        let serialized_len = metrics.serialized_len();
        let remaining_initial = buf.remaining_mut();

        if buf.remaining_mut() < serialized_len {
            Err(SerializeError::TooSmallForHeader)?
        }

        let mut initial = SUPPORTED_VERSION << VERSION_SHIFT;
        let marker = match self.marker {
            FrameMarker::Single => FRAME_MARKER_SINGLE,
            FrameMarker::Start => FRAME_MARKER_START,
            FrameMarker::Inter => FRAME_MARKER_INTER,
            FrameMarker::Final => FRAME_MARKER_FINAL,
        };
        initial |= marker << FRAME_MARKER_SHIFT;

        if metrics.ext_len > 0 {
            initial |= 1 << EXT_FLAG_SHIFT;
        }
        buf.put_u8(initial);
        buf.put_u8(0); // Reserved

        buf.put_u16(self.track_handle.into());
        buf.put_u16(self.sequence);
        buf.put_u16(self.frame_number);
        buf.put_u32(self.timestamp.as_ticks());

        if metrics.ext_len > 0 {
            buf.put_u16((metrics.ext_words - 1) as u16);
            self.extensions.serialize_into(buf);
            buf.put_bytes(0, metrics.padding_len);
        }

        assert_eq!(remaining_initial - buf.remaining_mut(), serialized_len);
        Ok(serialized_len)
    }
}

impl Extensions {
    /// Length of extensions excluding padding.
    fn serialized_len(&self) -> usize {
        let mut len = 0;
        if self.e2ee.is_some() {
            len += EXT_MARKER_LEN + E2eeExt::LEN;
        }
        if self.user_timestamp.is_some() {
            len += EXT_MARKER_LEN + UserTimestampExt::LEN;
        }
        len
    }

    fn serialize_into(self, buf: &mut impl BufMut) {
        if let Some(e2ee) = self.e2ee {
            e2ee.serialize_into(buf);
        }
        if let Some(user_timestamp) = self.user_timestamp {
            user_timestamp.serialize_into(buf);
        }
    }
}

impl E2eeExt {
    fn serialize_into(self, buf: &mut impl BufMut) {
        buf.put_u16(Self::TAG);
        buf.put_u16(Self::LEN as u16 - 1);
        buf.put_u8(self.key_index);
        buf.put_slice(&self.iv);
    }
}

impl UserTimestampExt {
    fn serialize_into(self, buf: &mut impl BufMut) {
        buf.put_u16(Self::TAG);
        buf.put_u16(Self::LEN as u16 - 1);
        buf.put_u64(self.0);
    }
}

#[cfg(test)]
mod tests {
    use crate::packet::{
        E2eeExt, Extensions, FrameMarker, Header, Packet, Timestamp, UserTimestampExt,
    };
    use bytes::Buf;

    /// Constructed packet to use in tests.
    fn packet() -> Packet {
        Packet {
            header: Header {
                marker: FrameMarker::Final,
                track_handle: 0x8811u32.try_into().unwrap(),
                sequence: 0x4422,
                frame_number: 0x4411,
                timestamp: Timestamp::from_ticks(0x44221188),
                extensions: Extensions {
                    user_timestamp: UserTimestampExt(0x4411221111118811).into(),
                    e2ee: E2eeExt { key_index: 0xFA, iv: [0x3C; 12] }.into(),
                },
            },
            payload: vec![0xFA; 1024].into(),
        }
    }

    #[test]
    fn test_header_metrics() {
        let metrics = packet().header.metrics();
        assert_eq!(metrics.ext_len, 29);
        assert_eq!(metrics.ext_words, 8);
        assert_eq!(metrics.padding_len, 3);
    }

    #[test]
    fn test_serialized_length() {
        let packet = packet();
        assert_eq!(packet.serialized_len(), 1070);
        assert_eq!(packet.header.serialized_len(), 46);
        assert_eq!(packet.header.extensions.serialized_len(), 29);
    }

    #[test]
    fn test_serialize() {
        let mut buf = packet().serialize().try_into_mut().unwrap();
        assert_eq!(buf.len(), 1070);

        // Base header
        assert_eq!(buf.get_u8(), 0xC); // Version 0, final, extension
        assert_eq!(buf.get_u8(), 0); // Reserved
        assert_eq!(buf.get_u16(), 0x8811); // Track handle
        assert_eq!(buf.get_u16(), 0x4422); // Sequence
        assert_eq!(buf.get_u16(), 0x4411); // Frame number
        assert_eq!(buf.get_u32(), 0x44221188); // Timestamp
        assert_eq!(buf.get_u16(), 7); // Extension words

        // E2EE extension
        assert_eq!(buf.get_u16(), 1); // ID 1,
        assert_eq!(buf.get_u16(), 12); // Length 12
        assert_eq!(buf.get_u8(), 0xFA); // Key index
        assert_eq!(buf.copy_to_bytes(12), vec![0x3C; 12]);

        // User timestamp extension
        assert_eq!(buf.get_u16(), 2); // ID 2
        assert_eq!(buf.get_u16(), 7); // Length 7
        assert_eq!(buf.get_u64(), 0x4411221111118811);

        assert_eq!(buf.copy_to_bytes(3), vec![0; 3]); // Padding
        assert_eq!(buf.copy_to_bytes(1024), vec![0xFA; 1024]); // Payload

        assert_eq!(buf.remaining(), 0);
    }
}
