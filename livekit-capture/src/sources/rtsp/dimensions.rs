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

//! Frame dimension parsing from keyframe payloads, for resolution discovery.
//!
//! Every parser returns `None` on malformed or unexpected input; discovery
//! treats that as "not discoverable" rather than a stream error.

use super::bits::{read_leb128, BitReader};
use crate::{
    encoded::{h26x::annex_b_nalus, EncodedVideoCodec},
    primitive::VideoResolution,
};

/// Extracts the frame dimensions from a keyframe access-unit payload.
pub(super) fn access_unit_resolution(
    codec: EncodedVideoCodec,
    payload: &[u8],
) -> Option<VideoResolution> {
    match codec {
        EncodedVideoCodec::H264 => annex_b_nalus(payload)
            .into_iter()
            .find(|nal| nal.first().is_some_and(|header| header & 0x1f == 7))
            .and_then(|nal| sps_resolution(codec, nal)),
        EncodedVideoCodec::H265 => annex_b_nalus(payload)
            .into_iter()
            .find(|nal| nal.first().is_some_and(|header| (header >> 1) & 0x3f == 33))
            .and_then(|nal| sps_resolution(codec, nal)),
        EncodedVideoCodec::VP8 => vp8_keyframe_resolution(payload),
        EncodedVideoCodec::VP9 => vp9_keyframe_resolution(payload),
        EncodedVideoCodec::AV1 => av1_sequence_header_resolution(payload),
    }
}

/// Extracts the frame dimensions from a raw H.264 or H.265 SPS NAL unit,
/// such as one carried out-of-band in SDP `sprop` attributes.
pub(super) fn sps_resolution(
    codec: EncodedVideoCodec,
    sps_nal: &[u8],
) -> Option<VideoResolution> {
    match codec {
        EncodedVideoCodec::H264 => h264_sps_resolution(&rbsp_from_nal(sps_nal, 1)?),
        EncodedVideoCodec::H265 => h265_sps_resolution(&rbsp_from_nal(sps_nal, 2)?),
        _ => None,
    }
}

/// Strips the NAL header and emulation-prevention bytes (`00 00 03`), which
/// must not reach the bit-level parsers.
fn rbsp_from_nal(nal: &[u8], header_len: usize) -> Option<Vec<u8>> {
    let payload = nal.get(header_len..)?;
    let mut rbsp = Vec::with_capacity(payload.len());
    let mut zeros = 0usize;
    for &byte in payload {
        if zeros >= 2 && byte == 0x03 {
            zeros = 0;
            continue;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        rbsp.push(byte);
    }
    Some(rbsp)
}

/// Parses the dimensions from an H.264 SPS RBSP (ITU-T H.264 section 7.3.2.1).
fn h264_sps_resolution(rbsp: &[u8]) -> Option<VideoResolution> {
    let mut reader = BitReader::new(rbsp);
    let profile_idc = reader.read_bits(8)?;
    reader.skip_bits(8)?; // constraint flags + reserved
    reader.skip_bits(8)?; // level_idc
    reader.read_ue()?; // seq_parameter_set_id

    let mut chroma_format_idc = 1;
    let mut separate_colour_plane = false;
    if matches!(profile_idc, 100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135)
    {
        chroma_format_idc = reader.read_ue()?;
        if chroma_format_idc == 3 {
            separate_colour_plane = reader.read_flag()?;
        }
        reader.read_ue()?; // bit_depth_luma_minus8
        reader.read_ue()?; // bit_depth_chroma_minus8
        reader.read_bit()?; // qpprime_y_zero_transform_bypass_flag
        if reader.read_flag()? {
            // seq_scaling_matrix_present_flag
            let lists = if chroma_format_idc == 3 { 12 } else { 8 };
            for index in 0..lists {
                if reader.read_flag()? {
                    skip_h264_scaling_list(&mut reader, if index < 6 { 16 } else { 64 })?;
                }
            }
        }
    }

    reader.read_ue()?; // log2_max_frame_num_minus4
    let pic_order_cnt_type = reader.read_ue()?;
    if pic_order_cnt_type == 0 {
        reader.read_ue()?; // log2_max_pic_order_cnt_lsb_minus4
    } else if pic_order_cnt_type == 1 {
        reader.read_bit()?; // delta_pic_order_always_zero_flag
        reader.skip_se()?; // offset_for_non_ref_pic
        reader.skip_se()?; // offset_for_top_to_bottom_field
        let cycle_length = reader.read_ue()?;
        for _ in 0..cycle_length {
            reader.skip_se()?; // offset_for_ref_frame
        }
    }
    reader.read_ue()?; // max_num_ref_frames
    reader.read_bit()?; // gaps_in_frame_num_value_allowed_flag

    let pic_width_in_mbs = reader.read_ue()?.checked_add(1)?;
    let pic_height_in_map_units = reader.read_ue()?.checked_add(1)?;
    let frame_mbs_only = reader.read_flag()?;
    if !frame_mbs_only {
        reader.read_bit()?; // mb_adaptive_frame_field_flag
    }
    reader.read_bit()?; // direct_8x8_inference_flag

    let frame_height_factor = if frame_mbs_only { 1 } else { 2 };
    let mut width = pic_width_in_mbs.checked_mul(16)?;
    let mut height =
        pic_height_in_map_units.checked_mul(16)?.checked_mul(frame_height_factor)?;

    if reader.read_flag()? {
        // frame_cropping_flag
        let crop_left = reader.read_ue()?;
        let crop_right = reader.read_ue()?;
        let crop_top = reader.read_ue()?;
        let crop_bottom = reader.read_ue()?;
        let chroma_array_type = if separate_colour_plane { 0 } else { chroma_format_idc };
        let (sub_width, sub_height) = match chroma_array_type {
            0 => (1, 1),
            1 => (2, 2),
            2 => (2, 1),
            _ => (1, 1),
        };
        let crop_unit_x = sub_width;
        let crop_unit_y = sub_height * frame_height_factor;
        width = width.checked_sub(crop_left.checked_add(crop_right)?.checked_mul(crop_unit_x)?)?;
        height =
            height.checked_sub(crop_top.checked_add(crop_bottom)?.checked_mul(crop_unit_y)?)?;
    }

    checked_resolution(width, height)
}

fn skip_h264_scaling_list(reader: &mut BitReader<'_>, size: u32) -> Option<()> {
    let mut last_scale = 8i32;
    let mut next_scale = 8i32;
    for _ in 0..size {
        if next_scale != 0 {
            let delta = read_se(reader)?;
            next_scale = (last_scale + delta).rem_euclid(256);
        }
        if next_scale != 0 {
            last_scale = next_scale;
        }
    }
    Some(())
}

fn read_se(reader: &mut BitReader<'_>) -> Option<i32> {
    let code = reader.read_ue()?;
    let magnitude = code.div_ceil(2) as i32;
    Some(if code % 2 == 1 { magnitude } else { -magnitude })
}

/// Parses the dimensions from an H.265 SPS RBSP (ITU-T H.265 section 7.3.2.2).
fn h265_sps_resolution(rbsp: &[u8]) -> Option<VideoResolution> {
    let mut reader = BitReader::new(rbsp);
    reader.skip_bits(4)?; // sps_video_parameter_set_id
    let max_sub_layers_minus1 = reader.read_bits(3)? as usize;
    reader.read_bit()?; // sps_temporal_id_nesting_flag
    skip_h265_profile_tier_level(&mut reader, max_sub_layers_minus1)?;
    reader.read_ue()?; // sps_seq_parameter_set_id

    let chroma_format_idc = reader.read_ue()?;
    if chroma_format_idc == 3 {
        reader.read_bit()?; // separate_colour_plane_flag
    }
    let mut width = reader.read_ue()?;
    let mut height = reader.read_ue()?;

    if reader.read_flag()? {
        // conformance_window_flag
        let left = reader.read_ue()?;
        let right = reader.read_ue()?;
        let top = reader.read_ue()?;
        let bottom = reader.read_ue()?;
        let (sub_width, sub_height) = match chroma_format_idc {
            1 => (2, 2),
            2 => (2, 1),
            _ => (1, 1),
        };
        width = width.checked_sub(left.checked_add(right)?.checked_mul(sub_width)?)?;
        height = height.checked_sub(top.checked_add(bottom)?.checked_mul(sub_height)?)?;
    }

    checked_resolution(width, height)
}

/// Skips a `profile_tier_level` structure with `profilePresentFlag = 1`.
fn skip_h265_profile_tier_level(
    reader: &mut BitReader<'_>,
    max_sub_layers_minus1: usize,
) -> Option<()> {
    reader.skip_bits(88)?; // general profile space/tier/idc/compat/constraints
    reader.skip_bits(8)?; // general_level_idc

    let mut profile_present = [false; 8];
    let mut level_present = [false; 8];
    for index in 0..max_sub_layers_minus1.min(8) {
        profile_present[index] = reader.read_flag()?;
        level_present[index] = reader.read_flag()?;
    }
    if max_sub_layers_minus1 > 0 {
        for _ in max_sub_layers_minus1..8 {
            reader.skip_bits(2)?; // reserved_zero_2bits
        }
    }
    for index in 0..max_sub_layers_minus1.min(8) {
        if profile_present[index] {
            reader.skip_bits(88)?;
        }
        if level_present[index] {
            reader.skip_bits(8)?;
        }
    }
    Some(())
}

/// Parses the dimensions from a VP8 keyframe payload (RFC 6386 section 9.1).
fn vp8_keyframe_resolution(payload: &[u8]) -> Option<VideoResolution> {
    let header = *payload.first()?;
    if header & 0x01 != 0 {
        return None; // not a keyframe
    }
    if payload.get(3..6)? != [0x9d, 0x01, 0x2a] {
        return None; // missing keyframe start code
    }
    let width = u32::from(u16::from_le_bytes([*payload.get(6)?, *payload.get(7)?]) & 0x3fff);
    let height = u32::from(u16::from_le_bytes([*payload.get(8)?, *payload.get(9)?]) & 0x3fff);
    checked_resolution(width, height)
}

/// Parses the dimensions from a VP9 keyframe's uncompressed header.
fn vp9_keyframe_resolution(payload: &[u8]) -> Option<VideoResolution> {
    let mut reader = BitReader::new(payload);
    if reader.read_bits(2)? != 0b10 {
        return None; // frame_marker
    }
    let profile = reader.read_bit()? | (reader.read_bit()? << 1);
    if profile == 3 {
        reader.read_bit()?; // reserved_zero
    }
    if reader.read_flag()? {
        return None; // show_existing_frame repeats a decoded frame
    }
    if reader.read_bit()? != 0 {
        return None; // frame_type: not a keyframe
    }
    reader.read_bit()?; // show_frame
    reader.read_bit()?; // error_resilient_mode
    if reader.read_bits(24)? != 0x49_83_42 {
        return None; // frame_sync_code
    }

    // color_config
    if profile >= 2 {
        reader.read_bit()?; // ten_or_twelve_bit
    }
    let color_space = reader.read_bits(3)?;
    const CS_RGB: u32 = 7;
    if color_space != CS_RGB {
        reader.read_bit()?; // color_range
        if profile == 1 || profile == 3 {
            reader.skip_bits(3)?; // subsampling_x, subsampling_y, reserved
        }
    } else if profile == 1 || profile == 3 {
        reader.read_bit()?; // reserved_zero
    }

    let width = reader.read_bits(16)?.checked_add(1)?;
    let height = reader.read_bits(16)?.checked_add(1)?;
    checked_resolution(width, height)
}

/// Parses the maximum frame dimensions from the sequence header OBU of an
/// AV1 access unit built from size-prefixed OBUs.
fn av1_sequence_header_resolution(payload: &[u8]) -> Option<VideoResolution> {
    let mut cursor = 0;
    while cursor < payload.len() {
        let header = *payload.get(cursor)?;
        if header & 0x80 != 0 {
            return None; // obu_forbidden_bit
        }
        let obu_type = (header & 0x78) >> 3;
        let has_extension = header & 0x04 != 0;
        let has_size = header & 0x02 != 0;
        cursor += if has_extension { 2 } else { 1 };
        if !has_size {
            // Without a size field the OBU extends to the end of the unit.
            return (obu_type == 1)
                .then(|| av1_sequence_header_obu_resolution(payload.get(cursor..)?))
                .flatten();
        }
        let size = read_leb128(payload, &mut cursor)?;
        let end = cursor.checked_add(size)?;
        let obu_payload = payload.get(cursor..end)?;
        if obu_type == 1 {
            return av1_sequence_header_obu_resolution(obu_payload);
        }
        cursor = end;
    }
    None
}

/// Parses `max_frame_width/height` from a sequence header OBU payload
/// (AV1 specification section 5.5.1).
fn av1_sequence_header_obu_resolution(payload: &[u8]) -> Option<VideoResolution> {
    let mut reader = BitReader::new(payload);
    reader.read_bits(3)?; // seq_profile
    reader.read_bit()?; // still_picture
    let reduced_still_picture_header = reader.read_flag()?;
    if reduced_still_picture_header {
        reader.skip_bits(5)?; // seq_level_idx[0]
    } else {
        let mut decoder_model_info_present = false;
        let mut buffer_delay_length = 0usize;
        if reader.read_flag()? {
            // timing_info_present_flag
            reader.skip_bits(32)?; // num_units_in_display_tick
            reader.skip_bits(32)?; // time_scale
            if reader.read_flag()? {
                // equal_picture_interval
                reader.read_ue()?; // num_ticks_per_picture_minus_1 (uvlc)
            }
            decoder_model_info_present = reader.read_flag()?;
            if decoder_model_info_present {
                buffer_delay_length = reader.read_bits(5)? as usize + 1;
                reader.skip_bits(32)?; // num_units_in_decoding_tick
                reader.skip_bits(10)?; // buffer_removal + frame_presentation lengths
            }
        }
        let initial_display_delay_present = reader.read_flag()?;
        let operating_points = reader.read_bits(5)? as usize + 1;
        for _ in 0..operating_points {
            reader.skip_bits(12)?; // operating_point_idc
            let seq_level_idx = reader.read_bits(5)?;
            if seq_level_idx > 7 {
                reader.read_bit()?; // seq_tier
            }
            if decoder_model_info_present && reader.read_flag()? {
                reader.skip_bits(buffer_delay_length * 2 + 1)?; // operating_parameters_info
            }
            if initial_display_delay_present && reader.read_flag()? {
                reader.skip_bits(4)?; // initial_display_delay_minus_1
            }
        }
    }

    let width_bits = reader.read_bits(4)? + 1;
    let height_bits = reader.read_bits(4)? + 1;
    let width = reader.read_bits(width_bits)?.checked_add(1)?;
    let height = reader.read_bits(height_bits)?.checked_add(1)?;
    checked_resolution(width, height)
}

fn checked_resolution(width: u32, height: u32) -> Option<VideoResolution> {
    (width > 0 && height > 0).then_some(VideoResolution::new(width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MSB-first bit writer for composing test bitstreams.
    #[derive(Default)]
    struct BitWriter {
        bytes: Vec<u8>,
        bit_offset: usize,
    }

    impl BitWriter {
        fn push_bit(&mut self, bit: u32) {
            if self.bit_offset % 8 == 0 {
                self.bytes.push(0);
            }
            let byte = self.bytes.last_mut().unwrap();
            *byte |= ((bit & 1) as u8) << (7 - self.bit_offset % 8);
            self.bit_offset += 1;
        }

        fn push_bits(&mut self, value: u32, bits: u32) {
            for offset in (0..bits).rev() {
                self.push_bit((value >> offset) & 1);
            }
        }

        fn push_ue(&mut self, value: u32) {
            let code = value + 1;
            let bits = 32 - code.leading_zeros();
            self.push_bits(0, bits - 1);
            self.push_bits(code, bits);
        }

        fn finish(mut self) -> Vec<u8> {
            // rbsp_stop_one_bit and alignment.
            self.push_bit(1);
            while self.bit_offset % 8 != 0 {
                self.push_bit(0);
            }
            self.bytes
        }
    }

    fn h264_sps_nal(profile_idc: u32, width: u32, height: u32, crop_bottom: u32) -> Vec<u8> {
        let mut writer = BitWriter::default();
        writer.push_bits(profile_idc, 8);
        writer.push_bits(0, 8); // constraint flags
        writer.push_bits(30, 8); // level_idc
        writer.push_ue(0); // seq_parameter_set_id
        if profile_idc == 100 {
            writer.push_ue(1); // chroma_format_idc (4:2:0)
            writer.push_ue(0); // bit_depth_luma_minus8
            writer.push_ue(0); // bit_depth_chroma_minus8
            writer.push_bit(0); // qpprime_y_zero_transform_bypass_flag
            writer.push_bit(0); // seq_scaling_matrix_present_flag
        }
        writer.push_ue(0); // log2_max_frame_num_minus4
        writer.push_ue(0); // pic_order_cnt_type
        writer.push_ue(0); // log2_max_pic_order_cnt_lsb_minus4
        writer.push_ue(1); // max_num_ref_frames
        writer.push_bit(0); // gaps_in_frame_num_value_allowed_flag
        writer.push_ue(width / 16 - 1);
        writer.push_ue(height.div_ceil(16) - 1);
        writer.push_bit(1); // frame_mbs_only_flag
        writer.push_bit(1); // direct_8x8_inference_flag
        if crop_bottom > 0 {
            writer.push_bit(1); // frame_cropping_flag
            writer.push_ue(0);
            writer.push_ue(0);
            writer.push_ue(0);
            writer.push_ue(crop_bottom / 2); // CropUnitY = 2 for 4:2:0
        } else {
            writer.push_bit(0);
        }
        writer.push_bit(0); // vui_parameters_present_flag

        let mut nal = vec![0x67];
        nal.extend(writer.finish());
        nal
    }

    fn h265_sps_nal(width: u32, height: u32, crop_bottom: u32) -> Vec<u8> {
        let mut writer = BitWriter::default();
        writer.push_bits(0, 4); // sps_video_parameter_set_id
        writer.push_bits(0, 3); // sps_max_sub_layers_minus1
        writer.push_bit(1); // sps_temporal_id_nesting_flag
        writer.push_bits(0, 32); // profile_tier_level: space/tier/idc/compat...
        writer.push_bits(0, 32);
        writer.push_bits(0, 24); // ...constraint and reserved bits (88 total)
        writer.push_bits(93, 8); // general_level_idc
        writer.push_ue(0); // sps_seq_parameter_set_id
        writer.push_ue(1); // chroma_format_idc (4:2:0)
        writer.push_ue(width);
        writer.push_ue(height + crop_bottom * 2);
        if crop_bottom > 0 {
            writer.push_bit(1); // conformance_window_flag
            writer.push_ue(0);
            writer.push_ue(0);
            writer.push_ue(0);
            writer.push_ue(crop_bottom); // SubHeightC = 2 for 4:2:0
        } else {
            writer.push_bit(0);
        }

        let mut nal = vec![0x42, 0x01];
        nal.extend(writer.finish());
        nal
    }

    #[test]
    fn parses_h264_sps_dimensions() {
        let nal = h264_sps_nal(66, 640, 480, 0);
        assert_eq!(
            sps_resolution(EncodedVideoCodec::H264, &nal),
            Some(VideoResolution::new(640, 480))
        );
    }

    #[test]
    fn parses_h264_high_profile_sps_with_cropping() {
        // 1920x1088 coded, cropped to 1920x1080.
        let nal = h264_sps_nal(100, 1920, 1080, 8);
        assert_eq!(
            sps_resolution(EncodedVideoCodec::H264, &nal),
            Some(VideoResolution::new(1920, 1080))
        );
    }

    #[test]
    fn parses_h264_sps_from_access_unit() {
        let sps = h264_sps_nal(66, 1280, 720, 0);
        let mut payload = Vec::new();
        payload.extend_from_slice(&[0, 0, 0, 1]);
        payload.extend_from_slice(&sps);
        payload.extend_from_slice(&[0, 0, 0, 1, 0x68, 0x08]);
        payload.extend_from_slice(&[0, 0, 0, 1, 0x65, 0x88, 0x84]);

        assert_eq!(
            access_unit_resolution(EncodedVideoCodec::H264, &payload),
            Some(VideoResolution::new(1280, 720))
        );
    }

    #[test]
    fn parses_h265_sps_dimensions() {
        let nal = h265_sps_nal(1280, 720, 0);
        assert_eq!(
            sps_resolution(EncodedVideoCodec::H265, &nal),
            Some(VideoResolution::new(1280, 720))
        );
    }

    #[test]
    fn parses_h265_sps_with_conformance_window() {
        // 1920x1088 coded, cropped to 1920x1080.
        let nal = h265_sps_nal(1920, 1080, 4);
        assert_eq!(
            sps_resolution(EncodedVideoCodec::H265, &nal),
            Some(VideoResolution::new(1920, 1080))
        );
    }

    #[test]
    fn strips_emulation_prevention_bytes() {
        assert_eq!(rbsp_from_nal(&[0x67, 0x01, 0x00, 0x00, 0x03, 0x02], 1), Some(vec![
            0x01, 0x00, 0x00, 0x02
        ]));
        // The escape only applies after two zero bytes.
        assert_eq!(rbsp_from_nal(&[0x67, 0x01, 0x00, 0x03, 0x02], 1), Some(vec![
            0x01, 0x00, 0x03, 0x02
        ]));
    }

    #[test]
    fn parses_vp8_keyframe_dimensions() {
        let payload = [
            0x00, 0x00, 0x00, // frame tag: keyframe
            0x9d, 0x01, 0x2a, // start code
            0x80, 0x02, // width 640
            0xe0, 0x01, // height 480
        ];
        assert_eq!(
            access_unit_resolution(EncodedVideoCodec::VP8, &payload),
            Some(VideoResolution::new(640, 480))
        );
    }

    #[test]
    fn rejects_vp8_delta_frame() {
        let payload = [0x01, 0x00, 0x00, 0x9d, 0x01, 0x2a, 0x80, 0x02, 0xe0, 0x01];
        assert_eq!(access_unit_resolution(EncodedVideoCodec::VP8, &payload), None);
    }

    #[test]
    fn parses_vp9_keyframe_dimensions() {
        let mut writer = BitWriter::default();
        writer.push_bits(0b10, 2); // frame_marker
        writer.push_bits(0, 2); // profile 0
        writer.push_bit(0); // show_existing_frame
        writer.push_bit(0); // frame_type: keyframe
        writer.push_bit(1); // show_frame
        writer.push_bit(0); // error_resilient_mode
        writer.push_bits(0x49_83_42, 24); // frame_sync_code
        writer.push_bits(0, 3); // color_space
        writer.push_bit(0); // color_range
        writer.push_bits(1280 - 1, 16);
        writer.push_bits(720 - 1, 16);

        assert_eq!(
            access_unit_resolution(EncodedVideoCodec::VP9, &writer.finish()),
            Some(VideoResolution::new(1280, 720))
        );
    }

    #[test]
    fn parses_av1_sequence_header_dimensions() {
        let mut writer = BitWriter::default();
        writer.push_bits(0, 3); // seq_profile
        writer.push_bit(0); // still_picture
        writer.push_bit(0); // reduced_still_picture_header
        writer.push_bit(0); // timing_info_present_flag
        writer.push_bit(0); // initial_display_delay_present_flag
        writer.push_bits(0, 5); // operating_points_cnt_minus_1
        writer.push_bits(0, 12); // operating_point_idc[0]
        writer.push_bits(5, 5); // seq_level_idx[0]
        writer.push_bits(10, 4); // frame_width_bits_minus_1
        writer.push_bits(9, 4); // frame_height_bits_minus_1
        writer.push_bits(1280 - 1, 11); // max_frame_width_minus_1
        writer.push_bits(720 - 1, 10); // max_frame_height_minus_1
        let obu_payload = writer.finish();

        // Size-prefixed sequence header OBU followed by an unrelated OBU.
        let mut payload = vec![0x0a, obu_payload.len() as u8];
        payload.extend(&obu_payload);
        payload.extend_from_slice(&[0x32, 0x01, 0x10]);

        assert_eq!(
            access_unit_resolution(EncodedVideoCodec::AV1, &payload),
            Some(VideoResolution::new(1280, 720))
        );
    }

    #[test]
    fn skips_leading_av1_obus_without_sequence_header() {
        let payload = [0x12, 0x00, 0x32, 0x01, 0x10]; // temporal delimiter + frame
        assert_eq!(access_unit_resolution(EncodedVideoCodec::AV1, &payload), None);
    }
}
