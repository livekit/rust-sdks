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

use bytes::Bytes;
use thiserror::Error;

use crate::{
    encoded::{
        h26x::access_unit_from_nalus, CodecSpecific, EncodedFrameType, EncodedVideoCodec,
        OwnedEncodedAccessUnit,
    },
    error::CaptureError,
};

/// Parsed RTP packet header and payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtpPacket<'a> {
    /// RTP marker bit.
    pub marker: bool,
    /// RTP payload type.
    pub payload_type: u8,
    /// RTP sequence number.
    pub sequence_number: u16,
    /// RTP timestamp.
    pub timestamp: u32,
    /// RTP SSRC.
    pub ssrc: u32,
    /// RTP payload bytes.
    pub payload: &'a [u8],
}

impl<'a> RtpPacket<'a> {
    /// Parses a single RTP packet.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, RtpDepacketizerError> {
        if bytes.len() < 12 {
            return Err(RtpDepacketizerError::PacketTooShort);
        }
        if bytes[0] >> 6 != 2 {
            return Err(RtpDepacketizerError::UnsupportedVersion(bytes[0] >> 6));
        }

        let has_padding = (bytes[0] & 0x20) != 0;
        let has_extension = (bytes[0] & 0x10) != 0;
        let csrc_count = (bytes[0] & 0x0f) as usize;
        let mut payload_start = 12 + csrc_count * 4;
        if bytes.len() < payload_start {
            return Err(RtpDepacketizerError::PacketTooShort);
        }

        if has_extension {
            if bytes.len() < payload_start + 4 {
                return Err(RtpDepacketizerError::PacketTooShort);
            }
            let extension_words =
                u16::from_be_bytes([bytes[payload_start + 2], bytes[payload_start + 3]]) as usize;
            payload_start += 4 + extension_words * 4;
            if bytes.len() < payload_start {
                return Err(RtpDepacketizerError::PacketTooShort);
            }
        }

        let payload_end = if has_padding {
            let Some(padding) = bytes.last().copied() else {
                return Err(RtpDepacketizerError::PacketTooShort);
            };
            let padding = padding as usize;
            if padding == 0 || bytes.len() < payload_start + padding {
                return Err(RtpDepacketizerError::PacketTooShort);
            }
            bytes.len() - padding
        } else {
            bytes.len()
        };

        Ok(Self {
            marker: (bytes[1] & 0x80) != 0,
            payload_type: bytes[1] & 0x7f,
            sequence_number: u16::from_be_bytes([bytes[2], bytes[3]]),
            timestamp: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            ssrc: u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            payload: &bytes[payload_start..payload_end],
        })
    }
}

/// Maps RTP timestamps to capture timestamps in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtpTimestampMapper {
    clock_rate: u32,
    last_rtp_timestamp: Option<u32>,
    extended_ticks: i64,
    base_timestamp_us: i64,
}

impl RtpTimestampMapper {
    /// Creates an RTP timestamp mapper.
    pub fn new(clock_rate: u32, base_timestamp_us: i64) -> Self {
        Self { clock_rate, last_rtp_timestamp: None, extended_ticks: 0, base_timestamp_us }
    }

    /// Maps an RTP timestamp to microseconds, unwrapping `u32` RTP timestamp
    /// rollover so mapped timestamps stay monotonic across any number of wraps.
    pub fn map(&mut self, rtp_timestamp: u32) -> Result<i64, RtpDepacketizerError> {
        if self.clock_rate == 0 {
            return Err(RtpDepacketizerError::InvalidClockRate);
        }

        let last = *self.last_rtp_timestamp.get_or_insert(rtp_timestamp);
        self.last_rtp_timestamp = Some(rtp_timestamp);
        // Reinterpreting the wrapped u32 delta as i32 picks the nearest extended
        // timestamp, which unwraps rollover while tolerating small backwards
        // jumps from reordered packets.
        let delta_ticks = i64::from(rtp_timestamp.wrapping_sub(last) as i32);
        self.extended_ticks = self.extended_ticks.saturating_add(delta_ticks);

        let extended_us = i128::from(self.extended_ticks) * 1_000_000 / i128::from(self.clock_rate);
        let extended_us = extended_us.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
        Ok(self.base_timestamp_us.saturating_add(extended_us))
    }
}

/// Error returned by RTP depayloading and access-unit assembly.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RtpDepacketizerError {
    /// RTP packet is shorter than its declared header.
    #[error("RTP packet is too short")]
    PacketTooShort,
    /// RTP version is not supported.
    #[error("unsupported RTP version {0}")]
    UnsupportedVersion(u8),
    /// RTP clock rate must be non-zero.
    #[error("RTP clock rate must be non-zero")]
    InvalidClockRate,
    /// RTP payload format is unsupported or malformed.
    #[error("unsupported or malformed RTP payload")]
    UnsupportedPayload,
    /// RTP fragmentation state was invalid.
    #[error("invalid RTP fragmentation sequence")]
    InvalidFragment,
    /// The payload descriptor is unsupported by the single-layer depacketizer.
    #[error("unsupported RTP payload descriptor")]
    UnsupportedPayloadDescriptor,
    /// Codec is not supported by this RTP assembler.
    #[error("RTP assembler does not support {0:?}")]
    UnsupportedCodec(EncodedVideoCodec),
    /// Capture data could not be converted into an access unit.
    #[error(transparent)]
    Capture(#[from] CaptureError),
}

/// Packet-loss recovery counters for an [`RtpAccessUnitAssembler`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RtpDepacketizerStats {
    /// Number of RTP sequence-number gaps detected.
    pub sequence_gaps: u64,
    /// Number of access units dropped while recovering from packet loss.
    pub dropped_access_units: u64,
    /// Whether output is gated until the next keyframe completes.
    pub awaiting_keyframe: bool,
}

/// Reassembles RTP packets into encoded access units.
#[derive(Debug, Clone)]
pub struct RtpAccessUnitAssembler {
    codec: EncodedVideoCodec,
    width: u32,
    height: u32,
    timestamp_mapper: RtpTimestampMapper,
    expected_sequence_number: Option<u16>,
    current: Option<PartialAccessUnit>,
    fragment: Option<FragmentState>,
    current_frame: Option<PartialFrame>,
    av1_fragment: Option<Av1FragmentState>,
    awaiting_keyframe: bool,
    sequence_gaps: u64,
    dropped_access_units: u64,
}

#[derive(Debug, Clone)]
struct PartialAccessUnit {
    rtp_timestamp: u32,
    timestamp_us: i64,
    nal_units: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
struct FragmentState {
    rtp_timestamp: u32,
    nal_unit: Vec<u8>,
}

#[derive(Debug, Clone)]
struct PartialFrame {
    rtp_timestamp: u32,
    timestamp_us: i64,
    payload: Vec<u8>,
    frame_type: Option<EncodedFrameType>,
    av1_reduced_still_picture_header: Option<bool>,
}

#[derive(Debug, Clone)]
struct Av1FragmentState {
    rtp_timestamp: u32,
    obu: Vec<u8>,
}

impl RtpAccessUnitAssembler {
    /// Creates an RTP access-unit assembler for supported video payloads.
    pub fn new(
        codec: EncodedVideoCodec,
        clock_rate: u32,
        start_timestamp_us: i64,
        width: u32,
        height: u32,
    ) -> Result<Self, RtpDepacketizerError> {
        if clock_rate == 0 {
            return Err(RtpDepacketizerError::InvalidClockRate);
        }

        Ok(Self {
            codec,
            width,
            height,
            timestamp_mapper: RtpTimestampMapper::new(clock_rate, start_timestamp_us),
            expected_sequence_number: None,
            current: None,
            fragment: None,
            current_frame: None,
            av1_fragment: None,
            awaiting_keyframe: false,
            sequence_gaps: 0,
            dropped_access_units: 0,
        })
    }

    /// Returns packet-loss recovery counters.
    pub fn stats(&self) -> RtpDepacketizerStats {
        RtpDepacketizerStats {
            sequence_gaps: self.sequence_gaps,
            dropped_access_units: self.dropped_access_units,
            awaiting_keyframe: self.awaiting_keyframe,
        }
    }

    /// Pushes one encoded RTP packet and returns an access unit when a marker closes a frame.
    pub fn push(
        &mut self,
        bytes: &[u8],
    ) -> Result<Option<OwnedEncodedAccessUnit>, RtpDepacketizerError> {
        let packet = RtpPacket::parse(bytes)?;
        self.push_packet(packet)
    }

    /// Pushes one parsed RTP packet and returns an access unit when a marker closes a frame.
    ///
    /// Packet loss is recovered internally: gaps and truncated fragments drop the
    /// interrupted access unit and gate output on the next keyframe instead of
    /// returning an error; see [`Self::stats`].
    pub fn push_packet(
        &mut self,
        packet: RtpPacket<'_>,
    ) -> Result<Option<OwnedEncodedAccessUnit>, RtpDepacketizerError> {
        self.check_sequence(packet.sequence_number);

        match self.codec {
            EncodedVideoCodec::H264 => self.push_h264_payload(&packet)?,
            EncodedVideoCodec::H265 => self.push_h265_payload(&packet)?,
            EncodedVideoCodec::VP8 => self.push_vp8_payload(&packet)?,
            EncodedVideoCodec::VP9 => self.push_vp9_payload(&packet)?,
            EncodedVideoCodec::AV1 => self.push_av1_payload(&packet)?,
        }

        if packet.marker {
            if self.fragment.is_some() || self.av1_fragment.is_some() {
                // The marker closed the access unit before the open fragment's
                // end arrived, so its tail packets were lost.
                self.discard_in_progress();
                self.dropped_access_units += 1;
                return Ok(None);
            }
            if matches!(
                self.codec,
                EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1
            ) {
                return self.finish_current_frame();
            }
            return self.finish_current();
        }
        Ok(None)
    }

    fn check_sequence(&mut self, sequence_number: u16) {
        let Some(expected) = self.expected_sequence_number.replace(sequence_number.wrapping_add(1))
        else {
            return;
        };
        if sequence_number == expected {
            return;
        }

        self.sequence_gaps += 1;
        self.discard_in_progress();
    }

    /// Discards all partially assembled state and gates output on the next keyframe.
    fn discard_in_progress(&mut self) {
        self.current = None;
        self.fragment = None;
        self.current_frame = None;
        self.av1_fragment = None;
        self.awaiting_keyframe = true;
    }

    /// Drops completed access units until a keyframe ends loss recovery.
    fn gate_on_keyframe(
        &mut self,
        access_unit: OwnedEncodedAccessUnit,
    ) -> Option<OwnedEncodedAccessUnit> {
        if self.awaiting_keyframe {
            if access_unit.frame_type != EncodedFrameType::Key {
                self.dropped_access_units += 1;
                return None;
            }
            self.awaiting_keyframe = false;
        }
        Some(access_unit)
    }

    fn current_mut(
        &mut self,
        rtp_timestamp: u32,
    ) -> Result<&mut PartialAccessUnit, RtpDepacketizerError> {
        if self.current.as_ref().is_some_and(|current| current.rtp_timestamp != rtp_timestamp) {
            self.current = None;
            self.fragment = None;
        }

        if self.current.is_none() {
            let timestamp_us = self.timestamp_mapper.map(rtp_timestamp)?;
            self.current =
                Some(PartialAccessUnit { rtp_timestamp, timestamp_us, nal_units: Vec::new() });
        }

        self.current.as_mut().ok_or(RtpDepacketizerError::InvalidFragment)
    }

    fn current_frame_mut(
        &mut self,
        rtp_timestamp: u32,
    ) -> Result<&mut PartialFrame, RtpDepacketizerError> {
        if self.current_frame.as_ref().is_some_and(|current| current.rtp_timestamp != rtp_timestamp)
        {
            self.current_frame = None;
            self.av1_fragment = None;
        }

        if self.current_frame.is_none() {
            let timestamp_us = self.timestamp_mapper.map(rtp_timestamp)?;
            self.current_frame = Some(PartialFrame {
                rtp_timestamp,
                timestamp_us,
                payload: Vec::new(),
                frame_type: None,
                av1_reduced_still_picture_header: None,
            });
        }

        self.current_frame.as_mut().ok_or(RtpDepacketizerError::InvalidFragment)
    }

    fn push_h264_payload(&mut self, packet: &RtpPacket<'_>) -> Result<(), RtpDepacketizerError> {
        let payload = packet.payload;
        let Some(&header) = payload.first() else {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        };
        let nal_type = header & 0x1f;

        match nal_type {
            1..=23 => self.current_mut(packet.timestamp)?.nal_units.push(payload.to_vec()),
            24 => self.push_h264_stap_a(packet.timestamp, &payload[1..])?,
            28 => self.push_h264_fu_a(packet.timestamp, payload)?,
            _ => return Err(RtpDepacketizerError::UnsupportedPayload),
        }

        Ok(())
    }

    fn push_h264_stap_a(
        &mut self,
        rtp_timestamp: u32,
        payload: &[u8],
    ) -> Result<(), RtpDepacketizerError> {
        let mut cursor = 0;
        while cursor < payload.len() {
            if payload.len() < cursor + 2 {
                return Err(RtpDepacketizerError::UnsupportedPayload);
            }
            let len = u16::from_be_bytes([payload[cursor], payload[cursor + 1]]) as usize;
            cursor += 2;
            if len == 0 || payload.len() < cursor + len {
                return Err(RtpDepacketizerError::UnsupportedPayload);
            }
            self.current_mut(rtp_timestamp)?.nal_units.push(payload[cursor..cursor + len].to_vec());
            cursor += len;
        }
        Ok(())
    }

    fn push_h264_fu_a(
        &mut self,
        rtp_timestamp: u32,
        payload: &[u8],
    ) -> Result<(), RtpDepacketizerError> {
        if payload.len() < 2 {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        }

        let indicator = payload[0];
        let header = payload[1];
        let start = (header & 0x80) != 0;
        let end = (header & 0x40) != 0;
        let nal_type = header & 0x1f;
        if nal_type == 0 || nal_type > 23 {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        }

        if start {
            let mut nal_unit = Vec::with_capacity(1 + payload.len().saturating_sub(2));
            nal_unit.push((indicator & 0xe0) | nal_type);
            nal_unit.extend_from_slice(&payload[2..]);
            self.fragment = Some(FragmentState { rtp_timestamp, nal_unit });
            return Ok(());
        }

        let Some(fragment) =
            self.fragment.as_mut().filter(|fragment| fragment.rtp_timestamp == rtp_timestamp)
        else {
            // A continuation without its start means the preceding packets were lost.
            self.discard_in_progress();
            return Ok(());
        };
        fragment.nal_unit.extend_from_slice(&payload[2..]);

        if end {
            let nal_unit =
                self.fragment.take().ok_or(RtpDepacketizerError::InvalidFragment)?.nal_unit;
            self.current_mut(rtp_timestamp)?.nal_units.push(nal_unit);
        }
        Ok(())
    }

    fn push_h265_payload(&mut self, packet: &RtpPacket<'_>) -> Result<(), RtpDepacketizerError> {
        let payload = packet.payload;
        if payload.len() < 2 {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        }
        let nal_type = (payload[0] >> 1) & 0x3f;

        match nal_type {
            0..=47 => self.current_mut(packet.timestamp)?.nal_units.push(payload.to_vec()),
            48 => self.push_h265_aggregation(packet.timestamp, &payload[2..])?,
            49 => self.push_h265_fragment(packet.timestamp, payload)?,
            _ => return Err(RtpDepacketizerError::UnsupportedPayload),
        }

        Ok(())
    }

    fn push_h265_aggregation(
        &mut self,
        rtp_timestamp: u32,
        payload: &[u8],
    ) -> Result<(), RtpDepacketizerError> {
        let mut cursor = 0;
        while cursor < payload.len() {
            if payload.len() < cursor + 2 {
                return Err(RtpDepacketizerError::UnsupportedPayload);
            }
            let len = u16::from_be_bytes([payload[cursor], payload[cursor + 1]]) as usize;
            cursor += 2;
            if len == 0 || payload.len() < cursor + len {
                return Err(RtpDepacketizerError::UnsupportedPayload);
            }
            self.current_mut(rtp_timestamp)?.nal_units.push(payload[cursor..cursor + len].to_vec());
            cursor += len;
        }
        Ok(())
    }

    fn push_h265_fragment(
        &mut self,
        rtp_timestamp: u32,
        payload: &[u8],
    ) -> Result<(), RtpDepacketizerError> {
        if payload.len() < 3 {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        }

        let fu_header = payload[2];
        let start = (fu_header & 0x80) != 0;
        let end = (fu_header & 0x40) != 0;
        let nal_type = fu_header & 0x3f;
        if nal_type > 47 {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        }

        if start {
            let mut nal_unit = Vec::with_capacity(2 + payload.len().saturating_sub(3));
            nal_unit.push((payload[0] & 0x81) | (nal_type << 1));
            nal_unit.push(payload[1]);
            nal_unit.extend_from_slice(&payload[3..]);
            self.fragment = Some(FragmentState { rtp_timestamp, nal_unit });
            return Ok(());
        }

        let Some(fragment) =
            self.fragment.as_mut().filter(|fragment| fragment.rtp_timestamp == rtp_timestamp)
        else {
            // A continuation without its start means the preceding packets were lost.
            self.discard_in_progress();
            return Ok(());
        };
        fragment.nal_unit.extend_from_slice(&payload[3..]);

        if end {
            let nal_unit =
                self.fragment.take().ok_or(RtpDepacketizerError::InvalidFragment)?.nal_unit;
            self.current_mut(rtp_timestamp)?.nal_units.push(nal_unit);
        }
        Ok(())
    }

    fn push_vp8_payload(&mut self, packet: &RtpPacket<'_>) -> Result<(), RtpDepacketizerError> {
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

    fn push_vp9_payload(&mut self, packet: &RtpPacket<'_>) -> Result<(), RtpDepacketizerError> {
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

    fn push_av1_payload(&mut self, packet: &RtpPacket<'_>) -> Result<(), RtpDepacketizerError> {
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

    fn finish_current(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, RtpDepacketizerError> {
        let Some(current) = self.current.take() else {
            return Ok(None);
        };
        if current.nal_units.is_empty() {
            return Ok(None);
        }

        let nal_units = current.nal_units.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let access_unit = access_unit_from_nalus(
            self.codec,
            &nal_units,
            current.timestamp_us,
            self.width,
            self.height,
        )?;
        Ok(self.gate_on_keyframe(access_unit))
    }

    fn finish_current_frame(
        &mut self,
    ) -> Result<Option<OwnedEncodedAccessUnit>, RtpDepacketizerError> {
        let Some(current) = self.current_frame.take() else {
            return Ok(None);
        };
        if current.payload.is_empty() {
            return Ok(None);
        }

        let mut access_unit = OwnedEncodedAccessUnit::new(
            self.codec,
            Bytes::from(current.payload),
            current.timestamp_us,
            current.frame_type.unwrap_or(EncodedFrameType::Delta),
            self.width,
            self.height,
        );
        access_unit.codec_specific = CodecSpecific::default_for(self.codec);
        Ok(self.gate_on_keyframe(access_unit))
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

#[derive(Debug, Clone)]
struct Av1PayloadDescriptor<'a> {
    starts_fragment: bool,
    ends_fragment: bool,
    elements: Vec<&'a [u8]>,
}

fn parse_vp8_payload_descriptor(
    payload: &[u8],
) -> Result<Vp8PayloadDescriptor<'_>, RtpDepacketizerError> {
    let Some(&descriptor) = payload.first() else {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    };
    let start_of_partition = descriptor & 0x10 != 0;
    let partition_id = descriptor & 0x0f;
    let mut cursor = 1;
    if descriptor & 0x80 != 0 {
        let Some(&extension) = payload.get(cursor) else {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        };
        cursor += 1;
        if extension & 0x80 != 0 {
            let Some(&picture_id) = payload.get(cursor) else {
                return Err(RtpDepacketizerError::UnsupportedPayload);
            };
            cursor += if picture_id & 0x80 != 0 { 2 } else { 1 };
        }
        if extension & 0x40 != 0 {
            cursor += 1;
        }
        if extension & 0x20 != 0 || extension & 0x10 != 0 {
            cursor += 1;
        }
    }
    if cursor > payload.len() {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    }
    Ok(Vp8PayloadDescriptor { start_of_partition, partition_id, payload: &payload[cursor..] })
}

fn parse_vp9_payload_descriptor(
    payload: &[u8],
) -> Result<Vp9PayloadDescriptor<'_>, RtpDepacketizerError> {
    let Some(&descriptor) = payload.first() else {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    };
    if descriptor & 0x10 != 0 {
        return Err(RtpDepacketizerError::UnsupportedPayloadDescriptor);
    }

    let beginning_of_frame = descriptor & 0x08 != 0;
    let inter_picture_predicted = descriptor & 0x40 != 0;
    let mut cursor = 1;
    if descriptor & 0x80 != 0 {
        let Some(&picture_id) = payload.get(cursor) else {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        };
        cursor += if picture_id & 0x80 != 0 { 2 } else { 1 };
    }

    let mut spatial_id = None;
    let mut inter_layer_predicted = None;
    if descriptor & 0x20 != 0 {
        let Some(&layer_info) = payload.get(cursor) else {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        };
        cursor += 1;
        spatial_id = Some((layer_info >> 1) & 0x07);
        inter_layer_predicted = Some(layer_info & 0x01 != 0);
        cursor += 1; // TL0PICIDX is present in non-flexible mode.
    }

    if descriptor & 0x02 != 0 {
        skip_vp9_scalability_structure(payload, &mut cursor)?;
    }

    if cursor > payload.len() {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    }
    Ok(Vp9PayloadDescriptor {
        beginning_of_frame,
        inter_picture_predicted,
        spatial_id,
        inter_layer_predicted,
        payload: &payload[cursor..],
    })
}

fn skip_vp9_scalability_structure(
    payload: &[u8],
    cursor: &mut usize,
) -> Result<(), RtpDepacketizerError> {
    let Some(&structure) = payload.get(*cursor) else {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    };
    *cursor += 1;

    let spatial_layers = ((structure >> 5) & 0x07) + 1;
    if spatial_layers != 1 {
        return Err(RtpDepacketizerError::UnsupportedPayloadDescriptor);
    }

    if structure & 0x10 != 0 {
        let bytes = usize::from(spatial_layers) * 4;
        skip_bytes(payload, cursor, bytes)?;
    }

    if structure & 0x08 != 0 {
        let Some(&group_count) = payload.get(*cursor) else {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        };
        *cursor += 1;
        for _ in 0..group_count {
            let Some(&group) = payload.get(*cursor) else {
                return Err(RtpDepacketizerError::UnsupportedPayload);
            };
            *cursor += 1;
            skip_bytes(payload, cursor, usize::from((group >> 2) & 0x03))?;
        }
    }

    Ok(())
}

fn skip_bytes(
    payload: &[u8],
    cursor: &mut usize,
    bytes: usize,
) -> Result<(), RtpDepacketizerError> {
    let Some(next) = cursor.checked_add(bytes) else {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    };
    if next > payload.len() {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    }
    *cursor = next;
    Ok(())
}

fn parse_av1_payload_descriptor(
    payload: &[u8],
) -> Result<Av1PayloadDescriptor<'_>, RtpDepacketizerError> {
    let Some(&header) = payload.first() else {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    };
    let starts_fragment = header & 0x80 != 0;
    let ends_fragment = header & 0x40 != 0;
    let element_count = (header >> 4) & 0x03;

    let mut cursor = 1;
    let mut elements = Vec::new();
    if element_count == 0 {
        while cursor < payload.len() {
            let len = read_leb128(payload, &mut cursor)?;
            let Some(end) = cursor.checked_add(len) else {
                return Err(RtpDepacketizerError::UnsupportedPayload);
            };
            if end > payload.len() {
                return Err(RtpDepacketizerError::UnsupportedPayload);
            }
            elements.push(&payload[cursor..end]);
            cursor = end;
        }
    } else {
        for index in 0..usize::from(element_count) {
            let len = if index + 1 == usize::from(element_count) {
                payload.len().saturating_sub(cursor)
            } else {
                read_leb128(payload, &mut cursor)?
            };
            let Some(end) = cursor.checked_add(len) else {
                return Err(RtpDepacketizerError::UnsupportedPayload);
            };
            if end > payload.len() {
                return Err(RtpDepacketizerError::UnsupportedPayload);
            }
            elements.push(&payload[cursor..end]);
            cursor = end;
        }
    }

    Ok(Av1PayloadDescriptor { starts_fragment, ends_fragment, elements })
}

fn read_leb128(bytes: &[u8], cursor: &mut usize) -> Result<usize, RtpDepacketizerError> {
    let mut value = 0usize;
    let mut shift = 0usize;
    loop {
        let Some(&byte) = bytes.get(*cursor) else {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        };
        *cursor += 1;
        value |= usize::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
        if shift >= usize::BITS as usize {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        }
    }
}

fn write_leb128(mut value: usize, out: &mut Vec<u8>) {
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

fn av1_obu_from_rtp_element(element: &[u8]) -> Result<Vec<u8>, RtpDepacketizerError> {
    let Some(&header) = element.first() else {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    };
    if header & 0x80 != 0 {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    }

    if header & 0x02 != 0 {
        let mut cursor = if header & 0x04 != 0 { 2 } else { 1 };
        if cursor > element.len() {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        }
        let payload_size = read_leb128(element, &mut cursor)?;
        if payload_size != element.len().saturating_sub(cursor) {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        }
        return Ok(element.to_vec());
    }

    let payload_offset = if header & 0x04 != 0 { 2 } else { 1 };
    if payload_offset > element.len() {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    }

    let payload_size = element.len() - payload_offset;
    let mut obu = Vec::with_capacity(element.len() + 8);
    obu.push(header | 0x02);
    if header & 0x04 != 0 {
        obu.push(element[1]);
    }
    write_leb128(payload_size, &mut obu);
    obu.extend_from_slice(&element[payload_offset..]);
    Ok(obu)
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

fn av1_reduced_still_picture_header(obu: &[u8]) -> Result<Option<bool>, RtpDepacketizerError> {
    let Some((obu_type, payload)) = av1_obu_parts(obu)? else {
        return Ok(None);
    };
    if obu_type != 1 {
        return Ok(None);
    }

    let mut reader = MsbBitReader::new(payload);
    reader.read_bits(3).ok_or(RtpDepacketizerError::UnsupportedPayload)?; // seq_profile
    reader.read_bit().ok_or(RtpDepacketizerError::UnsupportedPayload)?; // still_picture
    Ok(Some(reader.read_bit().ok_or(RtpDepacketizerError::UnsupportedPayload)? != 0))
}

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

    let mut reader = MsbBitReader::new(payload);
    let show_existing_frame = reader.read_bit().ok_or(RtpDepacketizerError::UnsupportedPayload)?;
    if show_existing_frame != 0 {
        return Ok(Some(EncodedFrameType::Delta));
    }

    let frame_type = reader.read_bits(2).ok_or(RtpDepacketizerError::UnsupportedPayload)?;
    Ok(Some(if frame_type == 0 { EncodedFrameType::Key } else { EncodedFrameType::Delta }))
}

fn av1_obu_parts(obu: &[u8]) -> Result<Option<(u8, &[u8])>, RtpDepacketizerError> {
    let Some(&header) = obu.first() else {
        return Ok(None);
    };
    if header & 0x80 != 0 {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    }

    let obu_type = av1_obu_type(obu).ok_or(RtpDepacketizerError::UnsupportedPayload)?;
    let has_extension = header & 0x04 != 0;
    let has_size = header & 0x02 != 0;
    let mut cursor = if has_extension { 2 } else { 1 };
    if cursor > obu.len() {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    }

    if !has_size {
        return Ok(Some((obu_type, &obu[cursor..])));
    }

    let payload_size = read_leb128(obu, &mut cursor)?;
    let Some(end) = cursor.checked_add(payload_size) else {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    };
    if end > obu.len() {
        return Err(RtpDepacketizerError::UnsupportedPayload);
    }
    Ok(Some((obu_type, &obu[cursor..end])))
}

struct MsbBitReader<'a> {
    bytes: &'a [u8],
    bit_offset: usize,
}

impl<'a> MsbBitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, bit_offset: 0 }
    }

    fn read_bit(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.bit_offset / 8)?;
        let bit = read_bit(byte, self.bit_offset % 8);
        self.bit_offset += 1;
        Some(bit)
    }

    fn read_bits(&mut self, bits: usize) -> Option<u8> {
        let mut value = 0;
        for _ in 0..bits {
            value = (value << 1) | self.read_bit()?;
        }
        Some(value)
    }
}

/// Reads bit `bit_offset` of `byte`, counting from the most significant bit.
fn read_bit(byte: u8, bit_offset: usize) -> u8 {
    (byte >> (7 - bit_offset)) & 0x01
}

fn av1_obu_type(obu: &[u8]) -> Option<u8> {
    obu.first().map(|header| (header & 0x78) >> 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rtp_packet(sequence_number: u16, timestamp: u32, marker: bool, payload: &[u8]) -> Vec<u8> {
        let mut packet = Vec::with_capacity(12 + payload.len());
        packet.push(0x80);
        packet.push(if marker { 0x80 | 96 } else { 96 });
        packet.extend_from_slice(&sequence_number.to_be_bytes());
        packet.extend_from_slice(&timestamp.to_be_bytes());
        packet.extend_from_slice(&0x1122_3344_u32.to_be_bytes());
        packet.extend_from_slice(payload);
        packet
    }

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
    fn parses_rtp_packet_header() {
        let bytes = rtp_packet(7, 90_000, true, &[0x65, 1, 2]);
        let packet = RtpPacket::parse(&bytes).unwrap();
        assert!(packet.marker);
        assert_eq!(packet.payload_type, 96);
        assert_eq!(packet.sequence_number, 7);
        assert_eq!(packet.timestamp, 90_000);
        assert_eq!(packet.payload, &[0x65, 1, 2]);
    }

    #[test]
    fn maps_rtp_timestamp_rollover() {
        let mut mapper = RtpTimestampMapper::new(90_000, 1_000);
        assert_eq!(mapper.map(u32::MAX - 89).unwrap(), 1_000);
        assert_eq!(mapper.map(0).unwrap(), 2_000);
    }

    #[test]
    fn maps_rtp_timestamps_across_multiple_rollovers() {
        let mut mapper = RtpTimestampMapper::new(90_000, 0);
        let step = 1u32 << 30;
        let mut rtp_timestamp = 0u32;
        let mut last_us = mapper.map(rtp_timestamp).unwrap();
        for _ in 0..20 {
            rtp_timestamp = rtp_timestamp.wrapping_add(step);
            let mapped_us = mapper.map(rtp_timestamp).unwrap();
            assert!(mapped_us > last_us, "mapped timestamps must stay monotonic");
            last_us = mapped_us;
        }
        assert_eq!(last_us, (20i64 << 30) * 1_000_000 / 90_000);
    }

    #[test]
    fn maps_reordered_rtp_timestamps() {
        let mut mapper = RtpTimestampMapper::new(90_000, 1_000);
        assert_eq!(mapper.map(9_000).unwrap(), 1_000);
        assert_eq!(mapper.map(18_000).unwrap(), 101_000);
        // A late packet maps behind the stream without disturbing what follows.
        assert_eq!(mapper.map(15_000).unwrap(), 67_666);
        assert_eq!(mapper.map(27_000).unwrap(), 201_000);
    }

    #[test]
    fn assembles_h264_fu_a() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::H264, 90_000, 0, 640, 480).unwrap();
        let start = rtp_packet(10, 12_000, false, &[0x7c, 0x85, 1, 2]);
        let end = rtp_packet(11, 12_000, true, &[0x7c, 0x45, 3, 4]);

        assert!(assembler.push(&start).unwrap().is_none());
        let access_unit = assembler.push(&end).unwrap().unwrap();
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2, 3, 4]);
    }

    #[test]
    fn sequence_gap_recovers_h264_at_next_keyframe() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::H264, 90_000, 0, 640, 480).unwrap();
        let start = rtp_packet(10, 12_000, false, &[0x7c, 0x85, 1, 2]);
        let delta = rtp_packet(12, 15_000, true, &[0x41, 1, 2]);
        let key = rtp_packet(13, 18_000, true, &[0x65, 3, 4]);

        assert!(assembler.push(&start).unwrap().is_none());
        // The gap dropped the fragment; the delta frame after it is withheld.
        assert!(assembler.push(&delta).unwrap().is_none());
        let stats = assembler.stats();
        assert_eq!(stats.sequence_gaps, 1);
        assert_eq!(stats.dropped_access_units, 1);
        assert!(stats.awaiting_keyframe);

        let access_unit = assembler.push(&key).unwrap().unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 3, 4]);
        let stats = assembler.stats();
        assert_eq!(stats.dropped_access_units, 1);
        assert!(!stats.awaiting_keyframe);
    }

    #[test]
    fn marker_with_open_h264_fragment_drops_access_unit() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::H264, 90_000, 0, 640, 480).unwrap();
        let start = rtp_packet(10, 12_000, false, &[0x7c, 0x85, 1, 2]);
        let truncated = rtp_packet(11, 12_000, true, &[0x7c, 0x05, 3, 4]);
        let key = rtp_packet(12, 15_000, true, &[0x65, 5, 6]);

        assert!(assembler.push(&start).unwrap().is_none());
        // The marker arrived without the FU end bit: the fragment is truncated.
        assert!(assembler.push(&truncated).unwrap().is_none());
        let stats = assembler.stats();
        assert_eq!(stats.sequence_gaps, 0);
        assert_eq!(stats.dropped_access_units, 1);
        assert!(stats.awaiting_keyframe);

        let access_unit = assembler.push(&key).unwrap().unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 5, 6]);
        assert!(!assembler.stats().awaiting_keyframe);
    }

    #[test]
    fn drops_h264_fu_continuation_without_start() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::H264, 90_000, 0, 640, 480).unwrap();
        let continuation = rtp_packet(10, 12_000, false, &[0x7c, 0x05, 1, 2]);
        let key = rtp_packet(11, 15_000, true, &[0x65, 3, 4]);

        assert!(assembler.push(&continuation).unwrap().is_none());
        assert!(assembler.stats().awaiting_keyframe);

        let access_unit = assembler.push(&key).unwrap().unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 3, 4]);
    }

    #[test]
    fn assembles_vp8_fragments() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::VP8, 90_000, 0, 640, 480).unwrap();
        let start = rtp_packet(10, 12_000, false, &[0x10, 0x00, 1, 2]);
        let end = rtp_packet(11, 12_000, true, &[0x00, 3, 4]);

        assert!(assembler.push(&start).unwrap().is_none());
        let access_unit = assembler.push(&end).unwrap().unwrap();
        assert_eq!(access_unit.codec, EncodedVideoCodec::VP8);
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x00, 1, 2, 3, 4]);
    }

    #[test]
    fn drops_vp8_mid_frame_start() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::VP8, 90_000, 0, 640, 480).unwrap();
        let mid_frame = rtp_packet(10, 12_000, true, &[0x00, 1, 2]);
        let key = rtp_packet(11, 15_000, true, &[0x10, 0x00, 3, 4]);

        assert!(assembler.push(&mid_frame).unwrap().is_none());
        assert!(assembler.stats().awaiting_keyframe);

        let access_unit = assembler.push(&key).unwrap().unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x00, 3, 4]);
    }

    #[test]
    fn assembles_vp9_single_layer_frame() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::VP9, 90_000, 0, 640, 480).unwrap();
        let packet = rtp_packet(10, 12_000, true, &[0x0c, 0x82, 1, 2]);

        let access_unit = assembler.push(&packet).unwrap().unwrap();
        assert_eq!(access_unit.codec, EncodedVideoCodec::VP9);
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x82, 1, 2]);
    }

    #[test]
    fn assembles_vp9_non_flexible_layer_descriptor() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::VP9, 90_000, 0, 640, 480).unwrap();
        let packet = rtp_packet(10, 12_000, true, &[0x2c, 0x10, 7, 0x82, 1, 2]);

        let access_unit = assembler.push(&packet).unwrap().unwrap();
        assert_eq!(access_unit.codec, EncodedVideoCodec::VP9);
        assert_eq!(access_unit.payload.as_ref(), &[0x82, 1, 2]);
    }

    #[test]
    fn assembles_vp9_single_layer_scalability_structure() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::VP9, 90_000, 0, 640, 480).unwrap();
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

        let access_unit = assembler.push(&packet).unwrap().unwrap();
        assert_eq!(access_unit.codec, EncodedVideoCodec::VP9);
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x82, 1, 2]);
    }

    #[test]
    fn assembles_vp9_descriptor_keyframe_from_prediction_bit() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::VP9, 90_000, 0, 640, 480).unwrap();
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

        let access_unit = assembler.push(&packet).unwrap().unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0xb1, 1, 2]);
    }

    #[test]
    fn assembles_vp9_predicted_frame_as_delta() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::VP9, 90_000, 0, 640, 480).unwrap();
        // P is set and the payload is an inter frame: must not classify as Key.
        let packet = rtp_packet(10, 12_000, true, &[0x4c, 0x86, 1, 2]);

        let access_unit = assembler.push(&packet).unwrap().unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Delta);
        assert_eq!(access_unit.payload.as_ref(), &[0x86, 1, 2]);
    }

    #[test]
    fn vp9_bitstream_keyframe_overrides_predicted_bit() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::VP9, 90_000, 0, 640, 480).unwrap();
        // P is set but the uncompressed header says KEY_FRAME.
        let packet = rtp_packet(10, 12_000, true, &[0x4c, 0x82, 1, 2]);

        let access_unit = assembler.push(&packet).unwrap().unwrap();
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
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::VP9, 90_000, 0, 640, 480).unwrap();
        let packet = rtp_packet(10, 12_000, true, &[0x0e, 0x20, 0x82, 1, 2]);

        let err = assembler.push(&packet).unwrap_err();
        assert_eq!(err, RtpDepacketizerError::UnsupportedPayloadDescriptor);
    }

    #[test]
    fn drops_vp9_mid_frame_start() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::VP9, 90_000, 0, 640, 480).unwrap();
        let mid_frame = rtp_packet(10, 12_000, true, &[0x04, 0x82, 1, 2]);
        let key = rtp_packet(11, 15_000, true, &[0x0c, 0x82, 3, 4]);

        assert!(assembler.push(&mid_frame).unwrap().is_none());
        assert!(assembler.stats().awaiting_keyframe);

        let access_unit = assembler.push(&key).unwrap().unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x82, 3, 4]);
    }

    #[test]
    fn rejects_vp9_flexible_mode() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::VP9, 90_000, 0, 640, 480).unwrap();
        let packet = rtp_packet(10, 12_000, true, &[0x1c, 0xa2, 1, 2]);

        let err = assembler.push(&packet).unwrap_err();
        assert_eq!(err, RtpDepacketizerError::UnsupportedPayloadDescriptor);
    }

    #[test]
    fn assembles_av1_temporal_unit() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::AV1, 90_000, 0, 640, 480).unwrap();
        let packet = rtp_packet(10, 12_000, true, &av1_sequence_and_frame_rtp_payload(0x10));

        let access_unit = assembler.push(&packet).unwrap().unwrap();
        assert_eq!(access_unit.codec, EncodedVideoCodec::AV1);
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x0a, 0x01, 0x00, 0x32, 0x01, 0x10]);
    }

    #[test]
    fn av1_sequence_header_before_inter_frame_is_delta() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::AV1, 90_000, 0, 640, 480).unwrap();
        let packet = rtp_packet(10, 12_000, true, &av1_sequence_and_frame_rtp_payload(0x38));

        let access_unit = assembler.push(&packet).unwrap().unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Delta);
        assert_eq!(access_unit.payload.as_ref(), &[0x0a, 0x01, 0x00, 0x32, 0x01, 0x38]);
    }

    #[test]
    fn assembles_fragmented_av1_obu() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::AV1, 90_000, 0, 640, 480).unwrap();
        let start = rtp_packet(10, 12_000, false, &[0x50, 0x30, 0x38]);
        let end = rtp_packet(11, 12_000, true, &[0x90, 2, 3]);

        assert!(assembler.push(&start).unwrap().is_none());
        let access_unit = assembler.push(&end).unwrap().unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Delta);
        assert_eq!(access_unit.payload.as_ref(), &[0x32, 0x03, 0x38, 2, 3]);
    }

    #[test]
    fn assembles_av1_obu_payload_with_size_field() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::AV1, 90_000, 0, 640, 480).unwrap();
        let packet = rtp_packet(10, 12_000, true, &[0x10, 0x30, 0x38, 2, 3]);

        let access_unit = assembler.push(&packet).unwrap().unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Delta);
        assert_eq!(access_unit.payload.as_ref(), &[0x32, 0x03, 0x38, 2, 3]);
    }

    #[test]
    fn marker_with_open_av1_fragment_drops_frame() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::AV1, 90_000, 0, 640, 480).unwrap();
        // Y is set, so the OBU fragment is unterminated when the marker closes it.
        let truncated = rtp_packet(10, 12_000, true, &[0x50, 0x30, 1]);
        let key = rtp_packet(11, 15_000, true, &av1_sequence_and_frame_rtp_payload(0x10));

        assert!(assembler.push(&truncated).unwrap().is_none());
        let stats = assembler.stats();
        assert_eq!(stats.dropped_access_units, 1);
        assert!(stats.awaiting_keyframe);

        let access_unit = assembler.push(&key).unwrap().unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x0a, 0x01, 0x00, 0x32, 0x01, 0x10]);
        assert!(!assembler.stats().awaiting_keyframe);
    }

    #[test]
    fn drops_av1_fragment_continuation_without_start() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::AV1, 90_000, 0, 640, 480).unwrap();
        // Z is set: this continues an OBU whose start was never received.
        let continuation = rtp_packet(10, 12_000, true, &[0x90, 2, 3]);
        let key = rtp_packet(11, 15_000, true, &av1_sequence_and_frame_rtp_payload(0x10));

        assert!(assembler.push(&continuation).unwrap().is_none());
        assert!(assembler.stats().awaiting_keyframe);

        let access_unit = assembler.push(&key).unwrap().unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x0a, 0x01, 0x00, 0x32, 0x01, 0x10]);
    }

    #[test]
    fn sequence_gap_recovers_vp8_at_next_keyframe() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::VP8, 90_000, 0, 640, 480).unwrap();
        let start = rtp_packet(10, 12_000, false, &[0x10, 0x00, 1, 2]);
        let delta = rtp_packet(12, 15_000, true, &[0x10, 0x01, 3, 4]);
        let key = rtp_packet(13, 18_000, true, &[0x10, 0x00, 5, 6]);

        assert!(assembler.push(&start).unwrap().is_none());
        // The gap dropped the fragment; the delta frame after it is withheld.
        assert!(assembler.push(&delta).unwrap().is_none());
        let stats = assembler.stats();
        assert_eq!(stats.sequence_gaps, 1);
        assert_eq!(stats.dropped_access_units, 1);
        assert!(stats.awaiting_keyframe);

        let access_unit = assembler.push(&key).unwrap().unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0x00, 5, 6]);
        assert!(!assembler.stats().awaiting_keyframe);
    }
}
