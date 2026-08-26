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

//! Bitstream readers shared by the RTP depacketizers and the codec header
//! parsers.

/// MSB-first bit reader over a byte slice. Every read returns `None` past
/// the end of the input.
#[derive(Debug, Clone)]
pub(super) struct BitReader<'a> {
    bytes: &'a [u8],
    bit_offset: usize,
}

impl<'a> BitReader<'a> {
    /// Creates a reader positioned at the first bit.
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, bit_offset: 0 }
    }

    /// Reads one bit.
    pub(super) fn read_bit(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.bit_offset / 8)?;
        let bit = (byte >> (7 - self.bit_offset % 8)) & 0x01;
        self.bit_offset += 1;
        Some(bit)
    }

    /// Reads one bit as a flag.
    pub(super) fn read_flag(&mut self) -> Option<bool> {
        self.read_bit().map(|bit| bit != 0)
    }

    /// Reads up to 32 bits MSB-first.
    pub(super) fn read_bits(&mut self, bits: u32) -> Option<u32> {
        debug_assert!(bits <= 32);
        let mut value = 0u32;
        for _ in 0..bits {
            value = (value << 1) | u32::from(self.read_bit()?);
        }
        Some(value)
    }

    /// Skips over `bits` bits.
    pub(super) fn skip_bits(&mut self, bits: usize) -> Option<()> {
        let next = self.bit_offset.checked_add(bits)?;
        if next > self.bytes.len() * 8 {
            return None;
        }
        self.bit_offset = next;
        Some(())
    }

    /// Reads an unsigned Exp-Golomb code (`ue(v)` in H.264/H.265).
    pub(super) fn read_ue(&mut self) -> Option<u32> {
        let mut leading_zeros = 0u32;
        while self.read_bit()? == 0 {
            leading_zeros += 1;
            if leading_zeros > 31 {
                return None;
            }
        }
        let suffix = self.read_bits(leading_zeros)?;
        (1u32 << leading_zeros).checked_sub(1)?.checked_add(suffix)
    }

    /// Reads a signed Exp-Golomb code (`se(v)` in H.264/H.265) and discards
    /// the value.
    pub(super) fn skip_se(&mut self) -> Option<()> {
        self.read_ue().map(|_| ())
    }
}

/// Reads an AV1/LEB128 length from `bytes` at `cursor`, advancing it.
pub(super) fn read_leb128(bytes: &[u8], cursor: &mut usize) -> Option<usize> {
    let mut value = 0usize;
    let mut shift = 0usize;
    loop {
        let &byte = bytes.get(*cursor)?;
        *cursor += 1;
        value |= usize::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
        if shift >= usize::BITS as usize {
            return None;
        }
    }
}

/// Appends `value` to `out` as LEB128.
pub(super) fn write_leb128(mut value: usize, out: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_bits_msb_first() {
        let mut reader = BitReader::new(&[0b1011_0001, 0b1000_0000]);
        assert_eq!(reader.read_bit(), Some(1));
        assert_eq!(reader.read_bits(3), Some(0b011));
        assert_eq!(reader.read_bits(5), Some(0b0001_1));
        assert!(reader.skip_bits(7).is_some());
        assert_eq!(reader.read_bit(), None);
    }

    #[test]
    fn reads_exp_golomb_codes() {
        // ue(v) codes: 1 -> 0, 010 -> 1, 011 -> 2, 00100 -> 3.
        let mut reader = BitReader::new(&[0b1010_0110, 0b0100_0000]);
        assert_eq!(reader.read_ue(), Some(0));
        assert_eq!(reader.read_ue(), Some(1));
        assert_eq!(reader.read_ue(), Some(2));
        assert_eq!(reader.read_ue(), Some(3));
    }

    #[test]
    fn exp_golomb_past_end_is_none() {
        let mut reader = BitReader::new(&[0b0000_0000]);
        assert_eq!(reader.read_ue(), None);
    }

    #[test]
    fn leb128_round_trips() {
        for value in [0usize, 1, 127, 128, 300, 16_383, 16_384, usize::from(u16::MAX)] {
            let mut encoded = Vec::new();
            write_leb128(value, &mut encoded);
            let mut cursor = 0;
            assert_eq!(read_leb128(&encoded, &mut cursor), Some(value));
            assert_eq!(cursor, encoded.len());
        }
    }

    #[test]
    fn leb128_rejects_truncated_input() {
        let mut cursor = 0;
        assert_eq!(read_leb128(&[0x80], &mut cursor), None);
    }
}
