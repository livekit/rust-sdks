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

//! Minimal keyframe detection for the five pre-encoded codecs supported by
//! [`EncodedTcpIngest`](super::encoded_tcp::EncodedTcpIngest).
//!
//! These probes are intentionally conservative — they never scan deeper into
//! a frame than needed to answer yes/no. Incorrect answers only affect
//! ingest-side stats and the `is_keyframe` flag forwarded to the
//! `NativeEncodedVideoSource`; WebRTC's own RTP packetizer recomputes what
//! it needs for keyframe signalling.

use libwebrtc::video_source::VideoCodec;

/// Returns the access-unit delimiter NAL type for Annex-B codecs. `None`
/// for codecs that are not delivered as Annex-B.
pub(super) fn aud_nal_type(codec: VideoCodec) -> Option<u8> {
    match codec {
        VideoCodec::H264 => Some(9),
        VideoCodec::H265 => Some(35),
        _ => None,
    }
}

/// Extracts the NAL unit type from the first byte after an Annex-B start
/// code. Returns 0 for codecs without NAL units.
pub(super) fn nal_type(codec: VideoCodec, first_byte: u8) -> u8 {
    match codec {
        VideoCodec::H264 => first_byte & 0x1F,
        VideoCodec::H265 => (first_byte >> 1) & 0x3F,
        _ => 0,
    }
}

/// Whether a given NAL type is a keyframe NAL.
///
/// * H.264: IDR slice (NAL type 5)
/// * H.265: any IRAP (BLA/IDR/CRA, NAL types 16..=23)
/// * VP8/VP9/AV1: never — they do not use NAL units.
pub(super) fn is_keyframe_nal(codec: VideoCodec, nal_type: u8) -> bool {
    match codec {
        VideoCodec::H264 => nal_type == 5,
        VideoCodec::H265 => (16..=23).contains(&nal_type),
        _ => false,
    }
}

/// Top-level keyframe probe. Delegates to codec-specific helpers.
///
/// * H.264 / H.265: scans for an IDR / IRAP NAL in the access unit.
/// * VP8: bit 0 of the frame tag (RFC 6386 §9.1: 0 = keyframe).
/// * VP9: decodes the leading bits of the uncompressed header (VP9 spec §6.2).
/// * AV1: scans OBUs in the Temporal Unit for an `OBU_SEQUENCE_HEADER`
///   (the same heuristic WebRTC's own AV1 RTP packetizer uses).
pub(super) fn is_keyframe(codec: VideoCodec, data: &[u8]) -> bool {
    match codec {
        VideoCodec::H264 | VideoCodec::H265 => is_keyframe_annex_b(codec, data),
        VideoCodec::Vp8 => !data.is_empty() && (data[0] & 0x01) == 0,
        VideoCodec::Vp9 => is_keyframe_vp9(data),
        VideoCodec::Av1 => is_keyframe_av1(data),
    }
}

fn is_keyframe_annex_b(codec: VideoCodec, data: &[u8]) -> bool {
    let mut i = 0usize;
    while i + 3 < data.len() {
        let is_four = i + 4 <= data.len()
            && data[i] == 0
            && data[i + 1] == 0
            && data[i + 2] == 0
            && data[i + 3] == 1;
        let is_three = data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1;
        if is_four || is_three {
            let payload_idx = if is_four { i + 4 } else { i + 3 };
            if payload_idx < data.len() && is_keyframe_nal(codec, nal_type(codec, data[payload_idx]))
            {
                return true;
            }
            i = payload_idx + 1;
        } else {
            i += 1;
        }
    }
    false
}

/// AV1 keyframe probe. Walks OBUs in a Temporal Unit and returns true if
/// any OBU has type `OBU_SEQUENCE_HEADER` (1). AV1 spec §5.3.2 (OBU header)
/// + §5.3.1 (leb128):
///
/// * byte 0 bits 6..=3: `obu_type`.
/// * byte 0 bit 2: `obu_extension_flag`; if set, one extension byte follows.
/// * byte 0 bit 1: `obu_has_size_field`; if set, a leb128-encoded `obu_size`
///   follows. If clear, the OBU runs to the end of the input and we cannot
///   skip it.
fn is_keyframe_av1(mut data: &[u8]) -> bool {
    const OBU_SEQUENCE_HEADER: u8 = 1;
    while !data.is_empty() {
        let header = data[0];
        let obu_type = (header >> 3) & 0x0F;
        let ext = (header & 0x04) != 0;
        let has_size = (header & 0x02) != 0;

        let mut off = 1;
        if ext {
            if off >= data.len() {
                return false;
            }
            off += 1;
        }
        if !has_size {
            return obu_type == OBU_SEQUENCE_HEADER;
        }
        let (size, size_len) = match read_leb128(&data[off..]) {
            Some(v) => v,
            None => return false,
        };
        off += size_len;
        let payload_end = match off.checked_add(size as usize) {
            Some(e) if e <= data.len() => e,
            _ => return false,
        };
        if obu_type == OBU_SEQUENCE_HEADER {
            return true;
        }
        data = &data[payload_end..];
    }
    false
}

/// Decodes an AV1 leb128 (unsigned little-endian base-128) integer.
/// Returns `(value, bytes_consumed)` or `None` on truncated input.
/// AV1 spec §4.10.5 caps the encoding at 8 bytes and 32 significant bits.
fn read_leb128(input: &[u8]) -> Option<(u32, usize)> {
    let mut value: u64 = 0;
    for (i, &byte) in input.iter().take(8).enumerate() {
        value |= ((byte & 0x7F) as u64) << (i * 7);
        if (byte & 0x80) == 0 {
            return u32::try_from(value).ok().map(|v| (v, i + 1));
        }
    }
    None
}

/// VP9 uncompressed-header keyframe probe. Reads first-byte bits (MSB
/// first) per VP9 bitstream spec §6.2:
///
/// * bits 7..=6: `frame_marker` (must be `0b10`).
/// * bit 5: `profile_low_bit`, bit 4: `profile_high_bit`
///   (combined `profile` ∈ 0..=3).
/// * For `profile == 3`: bit 3 is reserved-zero, bit 2 is
///   `show_existing_frame`, bit 1 is `frame_type`.
/// * For `profile != 3`: bit 3 is `show_existing_frame`, bit 2 is
///   `frame_type`.
///
/// A keyframe has `show_existing_frame == 0` and `frame_type == 0`.
fn is_keyframe_vp9(data: &[u8]) -> bool {
    let Some(&b0) = data.first() else {
        return false;
    };
    if (b0 >> 6) & 0b11 != 0b10 {
        return false;
    }
    let profile_low = (b0 >> 5) & 0x1;
    let profile_high = (b0 >> 4) & 0x1;
    let profile = (profile_high << 1) | profile_low;
    let (show_existing_bit, frame_type_bit) = if profile == 3 { (2, 1) } else { (3, 2) };
    let show_existing = (b0 >> show_existing_bit) & 0x1;
    if show_existing != 0 {
        return false;
    }
    let frame_type = (b0 >> frame_type_bit) & 0x1;
    frame_type == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h264_idr_is_keyframe() {
        // 4-byte start code + NAL header for IDR (type 5, nal_ref_idc=3): 0x65
        let data = [0x00, 0x00, 0x00, 0x01, 0x65, 0x88, 0x84];
        assert!(is_keyframe(VideoCodec::H264, &data));
    }

    #[test]
    fn h264_non_idr_not_keyframe() {
        // NAL header for non-IDR slice (type 1): 0x41
        let data = [0x00, 0x00, 0x00, 0x01, 0x41, 0x9a];
        assert!(!is_keyframe(VideoCodec::H264, &data));
    }

    #[test]
    fn h265_idr_w_radl_is_keyframe() {
        // H.265 NAL type 19 (IDR_W_RADL). NAL header byte is (type << 1): 0x26
        let data = [0x00, 0x00, 0x00, 0x01, 0x26, 0x01];
        assert!(is_keyframe(VideoCodec::H265, &data));
    }

    #[test]
    fn vp8_keyframe_bit_zero() {
        let kf = [0x00_u8];
        let pf = [0x01_u8];
        assert!(is_keyframe(VideoCodec::Vp8, &kf));
        assert!(!is_keyframe(VideoCodec::Vp8, &pf));
    }

    #[test]
    fn vp9_profile0_keyframe() {
        // frame_marker=10, profile=0 (both bits 0), show_existing=0, frame_type=0
        // => top bits 10 00 0 0 .. = 0b1000_0000 = 0x80
        let data = [0x80_u8];
        assert!(is_keyframe(VideoCodec::Vp9, &data));
    }

    #[test]
    fn vp9_profile0_interframe() {
        // frame_type bit = bit 2 => 0b1000_0100 = 0x84
        let data = [0x84_u8];
        assert!(!is_keyframe(VideoCodec::Vp9, &data));
    }

    #[test]
    fn av1_sequence_header_obu_is_keyframe() {
        // obu_type=1 (SEQUENCE_HEADER) => byte 0 = (1 << 3) | 0b010 = 0x0A
        // (obu_has_size_field=1, no extension). obu_size leb128 = 0 (one byte).
        let data = [0x0A, 0x00];
        assert!(is_keyframe(VideoCodec::Av1, &data));
    }

    #[test]
    fn av1_tile_group_obu_not_keyframe() {
        // obu_type=4 (TILE_GROUP), has_size=1. size=0.
        let data = [0x22, 0x00];
        assert!(!is_keyframe(VideoCodec::Av1, &data));
    }

    #[test]
    fn av1_leb128_single_byte() {
        assert_eq!(read_leb128(&[0x00]), Some((0, 1)));
        assert_eq!(read_leb128(&[0x7F]), Some((0x7F, 1)));
    }

    #[test]
    fn av1_leb128_multi_byte() {
        // 128 => 0x80, 0x01
        assert_eq!(read_leb128(&[0x80, 0x01]), Some((128, 2)));
    }
}
