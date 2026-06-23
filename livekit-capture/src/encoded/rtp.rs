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

use thiserror::Error;

use crate::{
    encoded::{h26x::access_unit_from_nalus, EncodedVideoCodec, OwnedEncodedAccessUnit},
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
    base_rtp_timestamp: Option<u32>,
    base_timestamp_us: i64,
}

impl RtpTimestampMapper {
    /// Creates an RTP timestamp mapper.
    pub fn new(clock_rate: u32, base_timestamp_us: i64) -> Self {
        Self { clock_rate, base_rtp_timestamp: None, base_timestamp_us }
    }

    /// Maps an RTP timestamp to microseconds, handling `u32` RTP timestamp rollover.
    pub fn map(&mut self, rtp_timestamp: u32) -> Result<i64, RtpDepacketizerError> {
        if self.clock_rate == 0 {
            return Err(RtpDepacketizerError::InvalidClockRate);
        }

        let base = *self.base_rtp_timestamp.get_or_insert(rtp_timestamp);
        let delta = rtp_timestamp.wrapping_sub(base) as u64;
        let delta_us = delta.saturating_mul(1_000_000) / u64::from(self.clock_rate);
        Ok(self.base_timestamp_us.saturating_add(delta_us as i64))
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
    /// RTP sequence number gap was detected.
    #[error("RTP sequence gap: expected {expected}, got {actual}")]
    SequenceGap {
        /// Expected RTP sequence number.
        expected: u16,
        /// Actual RTP sequence number.
        actual: u16,
    },
    /// RTP payload format is unsupported or malformed.
    #[error("unsupported or malformed RTP payload")]
    UnsupportedPayload,
    /// RTP fragmentation state was invalid.
    #[error("invalid RTP fragmentation sequence")]
    InvalidFragment,
    /// Codec is not supported by this RTP assembler.
    #[error("RTP assembler does not support {0:?}")]
    UnsupportedCodec(EncodedVideoCodec),
    /// Capture data could not be converted into an access unit.
    #[error(transparent)]
    Capture(#[from] CaptureError),
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

impl RtpAccessUnitAssembler {
    /// Creates an RTP access-unit assembler for H.264 or H.265 payloads.
    pub fn new(
        codec: EncodedVideoCodec,
        clock_rate: u32,
        start_timestamp_us: i64,
        width: u32,
        height: u32,
    ) -> Result<Self, RtpDepacketizerError> {
        match codec {
            EncodedVideoCodec::H264 | EncodedVideoCodec::H265 => {}
            EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
                return Err(RtpDepacketizerError::UnsupportedCodec(codec));
            }
        }
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
        })
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
    pub fn push_packet(
        &mut self,
        packet: RtpPacket<'_>,
    ) -> Result<Option<OwnedEncodedAccessUnit>, RtpDepacketizerError> {
        self.check_sequence(packet.sequence_number)?;

        match self.codec {
            EncodedVideoCodec::H264 => self.push_h264_payload(&packet)?,
            EncodedVideoCodec::H265 => self.push_h265_payload(&packet)?,
            EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
                return Err(RtpDepacketizerError::UnsupportedCodec(self.codec));
            }
        }

        if packet.marker {
            return self.finish_current();
        }
        Ok(None)
    }

    fn check_sequence(&mut self, sequence_number: u16) -> Result<(), RtpDepacketizerError> {
        let Some(expected) = self.expected_sequence_number.replace(sequence_number.wrapping_add(1))
        else {
            return Ok(());
        };
        if sequence_number == expected {
            return Ok(());
        }

        self.current = None;
        self.fragment = None;
        Err(RtpDepacketizerError::SequenceGap { expected, actual: sequence_number })
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

        let fragment = self
            .fragment
            .as_mut()
            .filter(|fragment| fragment.rtp_timestamp == rtp_timestamp)
            .ok_or(RtpDepacketizerError::InvalidFragment)?;
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

        let fragment = self
            .fragment
            .as_mut()
            .filter(|fragment| fragment.rtp_timestamp == rtp_timestamp)
            .ok_or(RtpDepacketizerError::InvalidFragment)?;
        fragment.nal_unit.extend_from_slice(&payload[3..]);

        if end {
            let nal_unit =
                self.fragment.take().ok_or(RtpDepacketizerError::InvalidFragment)?.nal_unit;
            self.current_mut(rtp_timestamp)?.nal_units.push(nal_unit);
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
        Ok(Some(access_unit_from_nalus(
            self.codec,
            &nal_units,
            current.timestamp_us,
            self.width,
            self.height,
        )?))
    }
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
    fn sequence_gap_clears_current_frame() {
        let mut assembler =
            RtpAccessUnitAssembler::new(EncodedVideoCodec::H264, 90_000, 0, 640, 480).unwrap();
        let start = rtp_packet(10, 12_000, false, &[0x7c, 0x85, 1, 2]);
        let end = rtp_packet(12, 12_000, true, &[0x7c, 0x45, 3, 4]);

        assert!(assembler.push(&start).unwrap().is_none());
        let err = assembler.push(&end).unwrap_err();
        assert_eq!(err, RtpDepacketizerError::SequenceGap { expected: 11, actual: 12 });
    }
}
