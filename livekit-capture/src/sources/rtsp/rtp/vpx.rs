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

//! VP8 (RFC 7741) and VP9 (draft-ietf-payload-vp9) RTP payload handling.
//!
//! Only single-layer streams are supported; scalable streams are rejected
//! with [`RtpDepacketizerError::UnsupportedPayloadDescriptor`].

use super::{RtpAccessUnitAssembler, RtpDepacketizerError, RtpPacket};
use crate::{encoded::EncodedFrameType, sources::rtsp::bits::ByteReader};

impl RtpAccessUnitAssembler {
    pub(super) fn push_vp8_payload(
        &mut self,
        packet: &RtpPacket<'_>,
    ) -> Result<(), RtpDepacketizerError> {
        let descriptor = parse_vp8_payload_descriptor(packet.payload)?;
        if descriptor.payload.is_empty() {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        }

        let frame = self.current_frame_mut(packet.timestamp)?;
        if frame.payload.is_empty() {
            if !descriptor.start_of_partition || descriptor.partition_id != 0 {
                // The beginning of this frame was lost.
                self.discard_in_progress();
                return Ok(());
            }
            frame.frame_type = Some(if is_vp8_keyframe(descriptor.payload) {
                EncodedFrameType::Key
            } else {
                EncodedFrameType::Delta
            });
        }
        frame.payload.extend_from_slice(descriptor.payload);
        Ok(())
    }

    pub(super) fn push_vp9_payload(
        &mut self,
        packet: &RtpPacket<'_>,
    ) -> Result<(), RtpDepacketizerError> {
        let descriptor = parse_vp9_payload_descriptor(packet.payload)?;
        if descriptor.payload.is_empty() {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        }
        if descriptor.spatial_id.unwrap_or(0) != 0
            || descriptor.inter_layer_predicted.unwrap_or(false)
        {
            return Err(RtpDepacketizerError::UnsupportedPayloadDescriptor);
        }

        let frame = self.current_frame_mut(packet.timestamp)?;
        if frame.payload.is_empty() {
            if !descriptor.beginning_of_frame {
                // The beginning of this frame was lost.
                self.discard_in_progress();
                return Ok(());
            }
            frame.frame_type = Some(
                if !descriptor.inter_picture_predicted || is_vp9_keyframe(descriptor.payload) {
                    EncodedFrameType::Key
                } else {
                    EncodedFrameType::Delta
                },
            );
        }
        frame.payload.extend_from_slice(descriptor.payload);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct Vp8PayloadDescriptor<'a> {
    start_of_partition: bool,
    partition_id: u8,
    payload: &'a [u8],
}

#[derive(Debug, Clone, Copy)]
struct Vp9PayloadDescriptor<'a> {
    beginning_of_frame: bool,
    inter_picture_predicted: bool,
    spatial_id: Option<u8>,
    inter_layer_predicted: Option<bool>,
    payload: &'a [u8],
}

fn parse_vp8_payload_descriptor(
    payload: &[u8],
) -> Result<Vp8PayloadDescriptor<'_>, RtpDepacketizerError> {
    let malformed = || RtpDepacketizerError::UnsupportedPayload;
    let mut reader = ByteReader::new(payload);

    let descriptor = reader.get_u8().ok_or_else(malformed)?;
    let start_of_partition = descriptor & 0x10 != 0;
    let partition_id = descriptor & 0x0f;
    if descriptor & 0x80 != 0 {
        let extension = reader.get_u8().ok_or_else(malformed)?;
        if extension & 0x80 != 0 {
            let picture_id = reader.get_u8().ok_or_else(malformed)?;
            if picture_id & 0x80 != 0 {
                reader.skip(1).ok_or_else(malformed)?;
            }
        }
        if extension & 0x40 != 0 {
            reader.skip(1).ok_or_else(malformed)?;
        }
        if extension & 0x20 != 0 || extension & 0x10 != 0 {
            reader.skip(1).ok_or_else(malformed)?;
        }
    }
    Ok(Vp8PayloadDescriptor { start_of_partition, partition_id, payload: reader.take_rest() })
}

fn parse_vp9_payload_descriptor(
    payload: &[u8],
) -> Result<Vp9PayloadDescriptor<'_>, RtpDepacketizerError> {
    let malformed = || RtpDepacketizerError::UnsupportedPayload;
    let mut reader = ByteReader::new(payload);

    let descriptor = reader.get_u8().ok_or_else(malformed)?;
    if descriptor & 0x10 != 0 {
        return Err(RtpDepacketizerError::UnsupportedPayloadDescriptor);
    }

    let beginning_of_frame = descriptor & 0x08 != 0;
    let inter_picture_predicted = descriptor & 0x40 != 0;
    if descriptor & 0x80 != 0 {
        let picture_id = reader.get_u8().ok_or_else(malformed)?;
        if picture_id & 0x80 != 0 {
            reader.skip(1).ok_or_else(malformed)?;
        }
    }

    let mut spatial_id = None;
    let mut inter_layer_predicted = None;
    if descriptor & 0x20 != 0 {
        let layer_info = reader.get_u8().ok_or_else(malformed)?;
        spatial_id = Some((layer_info >> 1) & 0x07);
        inter_layer_predicted = Some(layer_info & 0x01 != 0);
        reader.skip(1).ok_or_else(malformed)?; // TL0PICIDX in non-flexible mode
    }

    if descriptor & 0x02 != 0 {
        skip_vp9_scalability_structure(&mut reader)?;
    }

    Ok(Vp9PayloadDescriptor {
        beginning_of_frame,
        inter_picture_predicted,
        spatial_id,
        inter_layer_predicted,
        payload: reader.take_rest(),
    })
}

fn skip_vp9_scalability_structure(
    reader: &mut ByteReader<'_>,
) -> Result<(), RtpDepacketizerError> {
    let malformed = || RtpDepacketizerError::UnsupportedPayload;
    let structure = reader.get_u8().ok_or_else(malformed)?;

    let spatial_layers = ((structure >> 5) & 0x07) + 1;
    if spatial_layers != 1 {
        return Err(RtpDepacketizerError::UnsupportedPayloadDescriptor);
    }

    if structure & 0x10 != 0 {
        reader.skip(usize::from(spatial_layers) * 4).ok_or_else(malformed)?;
    }

    if structure & 0x08 != 0 {
        let group_count = reader.get_u8().ok_or_else(malformed)?;
        for _ in 0..group_count {
            let group = reader.get_u8().ok_or_else(malformed)?;
            reader.skip(usize::from((group >> 2) & 0x03)).ok_or_else(malformed)?;
        }
    }

    Ok(())
}

fn is_vp8_keyframe(payload: &[u8]) -> bool {
    payload.first().is_some_and(|header| header & 0x01 == 0)
}

/// Parses the start of a VP9 uncompressed frame header, whose `f(n)` fields
/// are MSB-first, and reports whether it begins a keyframe.
fn is_vp9_keyframe(payload: &[u8]) -> bool {
    let Some(&first_byte) = payload.first() else {
        return false;
    };
    // frame_marker: f(2), must be 0b10.
    if first_byte >> 6 != 0b10 {
        return false;
    }

    let mut bit_offset = 2usize;
    let profile_low = read_bit(first_byte, bit_offset);
    bit_offset += 1;
    let profile_high = read_bit(first_byte, bit_offset);
    bit_offset += 1;
    let profile = profile_low | (profile_high << 1);
    if profile == 3 {
        bit_offset += 1; // reserved_zero
    }
    // show_existing_frame: a repeated frame is never a keyframe.
    if read_bit(first_byte, bit_offset) != 0 {
        return false;
    }
    bit_offset += 1;
    // frame_type: 0 is KEY_FRAME.
    read_bit(first_byte, bit_offset) == 0
}

/// Reads bit `bit_offset` of `byte`, counting from the most significant bit.
fn read_bit(byte: u8, bit_offset: usize) -> u8 {
    (byte >> (7 - bit_offset)) & 0x01
}

#[cfg(test)]
mod tests {
    use super::super::tests::{assembler, push_one, rtp_packet};
    use super::*;
    use crate::encoded::EncodedVideoCodec;

    #[test]
    fn assembles_vp8_fragments() {
        let mut assembler = assembler(EncodedVideoCodec::VP8);
        let start = rtp_packet(10, 12_000, false, &[0x10, 0x00, 1, 2]);
        let end = rtp_packet(11, 12_000, true, &[0x00, 3, 4]);

        assert!(push_one(&mut assembler, &start).is_none());
        let access_unit = push_one(&mut assembler, &end).unwrap();
        assert_eq!(access_unit.codec, EncodedVideoCodec::VP8);
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x00, 1, 2, 3, 4]);
    }

    #[test]
    fn drops_vp8_mid_frame_start() {
        let mut assembler = assembler(EncodedVideoCodec::VP8);
        let mid_frame = rtp_packet(10, 12_000, true, &[0x00, 1, 2]);
        let key = rtp_packet(11, 15_000, true, &[0x10, 0x00, 3, 4]);

        assert!(push_one(&mut assembler, &mid_frame).is_none());
        assert!(assembler.stats().awaiting_keyframe);

        let access_unit = push_one(&mut assembler, &key).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x00, 3, 4]);
    }

    #[test]
    fn sequence_gap_recovers_vp8_at_next_keyframe() {
        let mut assembler = assembler(EncodedVideoCodec::VP8);
        let start = rtp_packet(10, 12_000, false, &[0x10, 0x00, 1, 2]);
        let delta = rtp_packet(12, 15_000, true, &[0x10, 0x01, 3, 4]);
        let key = rtp_packet(13, 18_000, true, &[0x10, 0x00, 5, 6]);

        assert!(push_one(&mut assembler, &start).is_none());
        // The gap dropped the fragment; the delta frame after it is withheld.
        assert!(push_one(&mut assembler, &delta).is_none());
        let stats = assembler.stats();
        assert_eq!(stats.sequence_gaps, 1);
        assert_eq!(stats.dropped_access_units, 1);
        assert!(stats.awaiting_keyframe);

        let access_unit = push_one(&mut assembler, &key).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x00, 5, 6]);
        assert!(!assembler.stats().awaiting_keyframe);
    }

    #[test]
    fn assembles_vp9_single_layer_frame() {
        let mut assembler = assembler(EncodedVideoCodec::VP9);
        let packet = rtp_packet(10, 12_000, true, &[0x0c, 0x82, 1, 2]);

        let access_unit = push_one(&mut assembler, &packet).unwrap();
        assert_eq!(access_unit.codec, EncodedVideoCodec::VP9);
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x82, 1, 2]);
    }

    #[test]
    fn assembles_vp9_non_flexible_layer_descriptor() {
        let mut assembler = assembler(EncodedVideoCodec::VP9);
        let packet = rtp_packet(10, 12_000, true, &[0x2c, 0x10, 7, 0x82, 1, 2]);

        let access_unit = push_one(&mut assembler, &packet).unwrap();
        assert_eq!(access_unit.codec, EncodedVideoCodec::VP9);
        assert_eq!(access_unit.payload.as_ref(), &[0x82, 1, 2]);
    }

    #[test]
    fn assembles_vp9_single_layer_scalability_structure() {
        let mut assembler = assembler(EncodedVideoCodec::VP9);
        let packet = rtp_packet(
            10,
            12_000,
            true,
            &[
                0x0e, // B, E, V
                0x18, // one spatial layer, resolution present, picture group present
                0x01, 0x40, 0x00, 0xb4, // 320x180
                0x01, // one picture group
                0x04, // one reference index
                0x01, // P_DIFF
                0x82, 1, 2,
            ],
        );

        let access_unit = push_one(&mut assembler, &packet).unwrap();
        assert_eq!(access_unit.codec, EncodedVideoCodec::VP9);
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x82, 1, 2]);
    }

    #[test]
    fn assembles_vp9_descriptor_keyframe_from_prediction_bit() {
        let mut assembler = assembler(EncodedVideoCodec::VP9);
        let packet = rtp_packet(
            10,
            12_000,
            true,
            &[
                0x0e, // B, E, V; P is clear, so this is not inter-picture predicted.
                0x18, // one spatial layer, resolution present, picture group present
                0x02, 0x80, 0x01, 0x68, // 640x360
                0x01, // one picture group
                0x04, // one reference index
                0x01, // P_DIFF
                0xb1, 1, 2,
            ],
        );

        let access_unit = push_one(&mut assembler, &packet).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0xb1, 1, 2]);
    }

    #[test]
    fn assembles_vp9_predicted_frame_as_delta() {
        let mut assembler = assembler(EncodedVideoCodec::VP9);
        // P is set and the payload is an inter frame: must not classify as Key.
        let packet = rtp_packet(10, 12_000, true, &[0x4c, 0x86, 1, 2]);

        let access_unit = push_one(&mut assembler, &packet).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Delta);
        assert_eq!(access_unit.payload.as_ref(), &[0x86, 1, 2]);
    }

    #[test]
    fn vp9_bitstream_keyframe_overrides_predicted_bit() {
        let mut assembler = assembler(EncodedVideoCodec::VP9);
        // P is set but the uncompressed header says KEY_FRAME.
        let packet = rtp_packet(10, 12_000, true, &[0x4c, 0x82, 1, 2]);

        let access_unit = push_one(&mut assembler, &packet).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x82, 1, 2]);
    }

    #[test]
    fn classifies_vp9_uncompressed_header_frame_types() {
        // 0b1000_0010: marker, profile 0, show_existing=0, KEY_FRAME, show_frame=1.
        assert!(is_vp9_keyframe(&[0x82]));
        // 0b1000_0011: keyframe with error_resilient_mode set.
        assert!(is_vp9_keyframe(&[0x83]));
        // 0b1011_0000: profile 3 keyframe.
        assert!(is_vp9_keyframe(&[0xb0]));
        // 0b1000_0110: frame_type=1, an inter frame.
        assert!(!is_vp9_keyframe(&[0x86]));
        // 0b1011_0010: profile 3 inter frame.
        assert!(!is_vp9_keyframe(&[0xb2]));
        // 0b1000_1000: show_existing_frame repeats a decoded frame.
        assert!(!is_vp9_keyframe(&[0x88]));
        // 0b0000_0010: invalid frame_marker.
        assert!(!is_vp9_keyframe(&[0x02]));
        assert!(!is_vp9_keyframe(&[]));
    }

    #[test]
    fn rejects_vp9_multi_layer_scalability_structure() {
        let mut assembler = assembler(EncodedVideoCodec::VP9);
        let packet = rtp_packet(10, 12_000, true, &[0x0e, 0x20, 0x82, 1, 2]);

        let err = assembler.push(&packet).unwrap_err();
        assert_eq!(err, RtpDepacketizerError::UnsupportedPayloadDescriptor);
    }

    #[test]
    fn drops_vp9_mid_frame_start() {
        let mut assembler = assembler(EncodedVideoCodec::VP9);
        let mid_frame = rtp_packet(10, 12_000, true, &[0x04, 0x82, 1, 2]);
        let key = rtp_packet(11, 15_000, true, &[0x0c, 0x82, 3, 4]);

        assert!(push_one(&mut assembler, &mid_frame).is_none());
        assert!(assembler.stats().awaiting_keyframe);

        let access_unit = push_one(&mut assembler, &key).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x82, 3, 4]);
    }

    #[test]
    fn rejects_vp9_flexible_mode() {
        let mut assembler = assembler(EncodedVideoCodec::VP9);
        let packet = rtp_packet(10, 12_000, true, &[0x1c, 0xa2, 1, 2]);

        let err = assembler.push(&packet).unwrap_err();
        assert_eq!(err, RtpDepacketizerError::UnsupportedPayloadDescriptor);
    }
}
