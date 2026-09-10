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

//! AV1 RTP payload handling (aomediacodec RTP payload specification).
//!
//! RTP carries OBU elements without size fields; assembly re-frames them as
//! size-prefixed OBUs so the access unit is a low-overhead AV1 bitstream.

use super::{Av1FragmentState, RtpAccessUnitAssembler, RtpDepacketizerError, RtpPacket};
use crate::{
    encoded::EncodedFrameType,
    sources::rtsp::bits::{write_leb128, BitReader, ByteReader},
};

impl RtpAccessUnitAssembler {
    pub(super) fn push_av1_payload(
        &mut self,
        packet: &RtpPacket<'_>,
    ) -> Result<(), RtpDepacketizerError> {
        let descriptor = parse_av1_payload_descriptor(packet.payload)?;
        if descriptor.elements.is_empty() {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        }

        let last_index = descriptor.elements.len() - 1;
        for (index, element) in descriptor.elements.iter().enumerate() {
            if element.is_empty() {
                return Err(RtpDepacketizerError::UnsupportedPayload);
            }

            let obu = if index == 0 && descriptor.starts_fragment {
                let Some(fragment) = self
                    .av1_fragment
                    .take()
                    .filter(|fragment| fragment.rtp_timestamp == packet.timestamp)
                else {
                    // A continuation without its start means the preceding packets were lost.
                    self.discard_in_progress();
                    return Ok(());
                };
                let mut obu = fragment.obu;
                obu.extend_from_slice(element);
                obu
            } else {
                if index == 0 && self.av1_fragment.is_some() {
                    return Err(RtpDepacketizerError::InvalidFragment);
                }
                element.to_vec()
            };

            if index == last_index && descriptor.ends_fragment {
                self.av1_fragment = Some(Av1FragmentState { rtp_timestamp: packet.timestamp, obu });
                return Ok(());
            }

            let mut obu = av1_obu_from_rtp_element(&obu)?;
            let frame = self.current_frame_mut(packet.timestamp)?;
            if let Some(reduced_still_picture_header) = av1_reduced_still_picture_header(&obu)? {
                frame.av1_reduced_still_picture_header = Some(reduced_still_picture_header);
            }
            if frame.frame_type.is_none() {
                frame.frame_type = av1_frame_type(&obu, frame.av1_reduced_still_picture_header)?;
            }
            frame.payload.append(&mut obu);
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct Av1PayloadDescriptor<'a> {
    starts_fragment: bool,
    ends_fragment: bool,
    elements: Vec<&'a [u8]>,
}

fn parse_av1_payload_descriptor(
    payload: &[u8],
) -> Result<Av1PayloadDescriptor<'_>, RtpDepacketizerError> {
    let malformed = || RtpDepacketizerError::UnsupportedPayload;
    let mut reader = ByteReader::new(payload);

    let header = reader.get_u8().ok_or_else(malformed)?;
    let starts_fragment = header & 0x80 != 0;
    let ends_fragment = header & 0x40 != 0;
    let element_count = usize::from((header >> 4) & 0x03);

    let mut elements = Vec::new();
    if element_count == 0 {
        while !reader.is_empty() {
            let len = reader.get_leb128().ok_or_else(malformed)?;
            elements.push(reader.take(len).ok_or_else(malformed)?);
        }
    } else {
        for index in 0..element_count {
            let element = if index + 1 == element_count {
                reader.take_rest()
            } else {
                let len = reader.get_leb128().ok_or_else(malformed)?;
                reader.take(len).ok_or_else(malformed)?
            };
            elements.push(element);
        }
    }

    Ok(Av1PayloadDescriptor { starts_fragment, ends_fragment, elements })
}

/// Converts an RTP OBU element into a size-prefixed OBU.
fn av1_obu_from_rtp_element(element: &[u8]) -> Result<Vec<u8>, RtpDepacketizerError> {
    let malformed = || RtpDepacketizerError::UnsupportedPayload;
    let Some(&header) = element.first() else {
        return Err(malformed());
    };
    if header & 0x80 != 0 {
        return Err(malformed());
    }
    let header_len = if header & 0x04 != 0 { 2 } else { 1 };

    if header & 0x02 != 0 {
        // Already size-prefixed; validate the size against the element.
        let mut reader = ByteReader::new(element);
        reader.skip(header_len).ok_or_else(malformed)?;
        let payload_size = reader.get_leb128().ok_or_else(malformed)?;
        if payload_size != reader.take_rest().len() {
            return Err(malformed());
        }
        return Ok(element.to_vec());
    }

    let payload = element.get(header_len..).ok_or_else(malformed)?;
    let mut obu = Vec::with_capacity(element.len() + 8);
    obu.push(header | 0x02);
    if header & 0x04 != 0 {
        obu.push(element[1]);
    }
    write_leb128(payload.len(), &mut obu);
    obu.extend_from_slice(payload);
    Ok(obu)
}

/// Reads `reduced_still_picture_header` from a sequence header OBU.
fn av1_reduced_still_picture_header(obu: &[u8]) -> Result<Option<bool>, RtpDepacketizerError> {
    let Some((obu_type, payload)) = av1_obu_parts(obu)? else {
        return Ok(None);
    };
    if obu_type != 1 {
        return Ok(None);
    }

    let mut reader = BitReader::new(payload);
    reader.read_bits(3).ok_or(RtpDepacketizerError::UnsupportedPayload)?; // seq_profile
    reader.read_bit().ok_or(RtpDepacketizerError::UnsupportedPayload)?; // still_picture
    Ok(Some(reader.read_bit().ok_or(RtpDepacketizerError::UnsupportedPayload)? != 0))
}

/// Classifies a frame or frame-header OBU, when `obu` is one.
fn av1_frame_type(
    obu: &[u8],
    reduced_still_picture_header: Option<bool>,
) -> Result<Option<EncodedFrameType>, RtpDepacketizerError> {
    let Some((obu_type, payload)) = av1_obu_parts(obu)? else {
        return Ok(None);
    };
    if !matches!(obu_type, 3 | 6) {
        return Ok(None);
    }

    if reduced_still_picture_header.unwrap_or(false) {
        return Ok(Some(EncodedFrameType::Key));
    }

    let mut reader = BitReader::new(payload);
    let show_existing_frame = reader.read_bit().ok_or(RtpDepacketizerError::UnsupportedPayload)?;
    if show_existing_frame != 0 {
        return Ok(Some(EncodedFrameType::Delta));
    }

    let frame_type = reader.read_bits(2).ok_or(RtpDepacketizerError::UnsupportedPayload)?;
    Ok(Some(if frame_type == 0 { EncodedFrameType::Key } else { EncodedFrameType::Delta }))
}

/// Splits an OBU into its type and payload bytes.
fn av1_obu_parts(obu: &[u8]) -> Result<Option<(u8, &[u8])>, RtpDepacketizerError> {
    let malformed = || RtpDepacketizerError::UnsupportedPayload;
    let Some(&header) = obu.first() else {
        return Ok(None);
    };
    if header & 0x80 != 0 {
        return Err(malformed());
    }

    let obu_type = (header & 0x78) >> 3;
    let has_extension = header & 0x04 != 0;
    let has_size = header & 0x02 != 0;
    let mut reader = ByteReader::new(obu);
    reader.skip(if has_extension { 2 } else { 1 }).ok_or_else(malformed)?;

    if !has_size {
        return Ok(Some((obu_type, reader.take_rest())));
    }

    let payload_size = reader.get_leb128().ok_or_else(malformed)?;
    let payload = reader.take(payload_size).ok_or_else(malformed)?;
    Ok(Some((obu_type, payload)))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{assembler, push_one, rtp_packet};
    use super::*;
    use crate::encoded::EncodedVideoCodec;

    fn av1_sequence_and_frame_rtp_payload(frame_header: u8) -> [u8; 6] {
        [
            0x28, // W=2, N=1.
            0x02, // First OBU element length.
            0x08, // Sequence header OBU without the size field.
            0x00, // profile=0, still_picture=false, reduced_still_picture_header=false.
            0x30, // Frame OBU without the size field.
            frame_header,
        ]
    }

    #[test]
    fn assembles_av1_temporal_unit() {
        let mut assembler = assembler(EncodedVideoCodec::AV1);
        let packet = rtp_packet(10, 12_000, true, &av1_sequence_and_frame_rtp_payload(0x10));

        let access_unit = push_one(&mut assembler, &packet).unwrap();
        assert_eq!(access_unit.codec, EncodedVideoCodec::AV1);
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x0a, 0x01, 0x00, 0x32, 0x01, 0x10]);
    }

    #[test]
    fn av1_sequence_header_before_inter_frame_is_delta() {
        let mut assembler = assembler(EncodedVideoCodec::AV1);
        let packet = rtp_packet(10, 12_000, true, &av1_sequence_and_frame_rtp_payload(0x38));

        let access_unit = push_one(&mut assembler, &packet).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Delta);
        assert_eq!(access_unit.payload.as_ref(), &[0x0a, 0x01, 0x00, 0x32, 0x01, 0x38]);
    }

    #[test]
    fn assembles_fragmented_av1_obu() {
        let mut assembler = assembler(EncodedVideoCodec::AV1);
        let start = rtp_packet(10, 12_000, false, &[0x50, 0x30, 0x38]);
        let end = rtp_packet(11, 12_000, true, &[0x90, 2, 3]);

        assert!(push_one(&mut assembler, &start).is_none());
        let access_unit = push_one(&mut assembler, &end).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Delta);
        assert_eq!(access_unit.payload.as_ref(), &[0x32, 0x03, 0x38, 2, 3]);
    }

    #[test]
    fn assembles_av1_obu_payload_with_size_field() {
        let mut assembler = assembler(EncodedVideoCodec::AV1);
        let packet = rtp_packet(10, 12_000, true, &[0x10, 0x30, 0x38, 2, 3]);

        let access_unit = push_one(&mut assembler, &packet).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Delta);
        assert_eq!(access_unit.payload.as_ref(), &[0x32, 0x03, 0x38, 2, 3]);
    }

    #[test]
    fn marker_with_open_av1_fragment_drops_frame() {
        let mut assembler = assembler(EncodedVideoCodec::AV1);
        // Y is set, so the OBU fragment is unterminated when the marker closes it.
        let truncated = rtp_packet(10, 12_000, true, &[0x50, 0x30, 1]);
        let key = rtp_packet(11, 15_000, true, &av1_sequence_and_frame_rtp_payload(0x10));

        assert!(push_one(&mut assembler, &truncated).is_none());
        let stats = assembler.stats();
        assert_eq!(stats.dropped_access_units, 1);
        assert!(stats.awaiting_keyframe);

        let access_unit = push_one(&mut assembler, &key).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x0a, 0x01, 0x00, 0x32, 0x01, 0x10]);
        assert!(!assembler.stats().awaiting_keyframe);
    }

    #[test]
    fn drops_av1_fragment_continuation_without_start() {
        let mut assembler = assembler(EncodedVideoCodec::AV1);
        // Z is set: this continues an OBU whose start was never received.
        let continuation = rtp_packet(10, 12_000, true, &[0x90, 2, 3]);
        let key = rtp_packet(11, 15_000, true, &av1_sequence_and_frame_rtp_payload(0x10));

        assert!(push_one(&mut assembler, &continuation).is_none());
        assert!(assembler.stats().awaiting_keyframe);

        let access_unit = push_one(&mut assembler, &key).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x0a, 0x01, 0x00, 0x32, 0x01, 0x10]);
    }
}
