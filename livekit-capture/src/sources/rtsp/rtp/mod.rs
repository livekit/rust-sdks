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

//! RTP depacketization into encoded access units.
//!
//! [`RtpAccessUnitAssembler`] reassembles the RTP payload formats of the
//! supported codecs and recovers from packet loss by discarding the
//! interrupted access unit and gating output on the next keyframe.

mod av1;
mod h26x;
mod vpx;

use std::collections::VecDeque;

use thiserror::Error;

use crate::{
    encoded::{
        h26x::{H26xParseError, MAX_PENDING_ACCESS_UNIT_BYTES},
        EncodedFrameType, EncodedVideoCodec, OwnedEncodedAccessUnit,
    },
    primitive::VideoResolution,
};

/// Out-of-band H.26x parameter sets, decoded from SDP `fmtp` attributes.
///
/// The assembler prepends missing parameter sets to keyframe access units so
/// every published keyframe is self-contained; see
/// [`RtpAccessUnitAssembler::finish_current`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct H26xParameterSets {
    /// H.265 video parameter sets (NAL type 32).
    pub(super) vps: Vec<Vec<u8>>,
    /// Sequence parameter sets (H.264 NAL type 7, H.265 NAL type 33).
    pub(super) sps: Vec<Vec<u8>>,
    /// Picture parameter sets (H.264 NAL type 8, H.265 NAL type 34).
    pub(super) pps: Vec<Vec<u8>>,
}

impl H26xParameterSets {
    /// Returns `true` when no parameter sets were provided.
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.vps.is_empty() && self.sps.is_empty() && self.pps.is_empty()
    }
}

/// Parsed RTP packet header and payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RtpPacket<'a> {
    /// RTP marker bit.
    pub(super) marker: bool,
    /// RTP payload type.
    pub(super) payload_type: u8,
    /// RTP sequence number.
    pub(super) sequence_number: u16,
    /// RTP timestamp.
    pub(super) timestamp: u32,
    /// RTP SSRC.
    pub(super) ssrc: u32,
    /// RTP payload bytes.
    pub(super) payload: &'a [u8],
}

impl<'a> RtpPacket<'a> {
    /// Parses a single RTP packet.
    pub(super) fn parse(bytes: &'a [u8]) -> Result<Self, RtpDepacketizerError> {
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
struct RtpTimestampMapper {
    clock_rate: u32,
    last_rtp_timestamp: Option<u32>,
    extended_ticks: i64,
    base_timestamp_us: i64,
}

impl RtpTimestampMapper {
    /// Creates an RTP timestamp mapper with a non-zero clock rate.
    fn new(clock_rate: u32, base_timestamp_us: i64) -> Result<Self, RtpDepacketizerError> {
        if clock_rate == 0 {
            return Err(RtpDepacketizerError::InvalidClockRate);
        }
        Ok(Self { clock_rate, last_rtp_timestamp: None, extended_ticks: 0, base_timestamp_us })
    }

    /// Maps an RTP timestamp to microseconds, unwrapping `u32` RTP timestamp
    /// rollover so mapped timestamps stay monotonic across any number of wraps.
    fn map(&mut self, rtp_timestamp: u32) -> i64 {
        let last = *self.last_rtp_timestamp.get_or_insert(rtp_timestamp);
        self.last_rtp_timestamp = Some(rtp_timestamp);
        // Reinterpreting the wrapped u32 delta as i32 picks the nearest extended
        // timestamp, which unwraps rollover while tolerating small backwards
        // jumps from reordered packets.
        let delta_ticks = i64::from(rtp_timestamp.wrapping_sub(last) as i32);
        self.extended_ticks = self.extended_ticks.saturating_add(delta_ticks);

        let extended_us = i128::from(self.extended_ticks) * 1_000_000 / i128::from(self.clock_rate);
        let extended_us = extended_us.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
        self.base_timestamp_us.saturating_add(extended_us)
    }
}

/// Error returned by RTP depacketization and access-unit assembly.
#[derive(Debug, Error, PartialEq, Eq)]
pub(super) enum RtpDepacketizerError {
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
    /// Assembled NAL units could not form an access unit.
    #[error(transparent)]
    H26x(#[from] H26xParseError),
}

/// Reassembles the RTP packets of one video stream into encoded access units.
///
/// Packets whose payload type or SSRC does not match the negotiated stream
/// are ignored. Packet loss is recovered internally: gaps and truncated
/// fragments drop the interrupted access unit and gate output on the next
/// keyframe instead of returning an error.
#[derive(Debug, Clone)]
pub(super) struct RtpAccessUnitAssembler {
    codec: EncodedVideoCodec,
    payload_type: u8,
    /// SSRC latched from the first matching packet; later packets from other
    /// sources on the same channel are dropped.
    ssrc: Option<u32>,
    resolution: VideoResolution,
    parameter_sets: H26xParameterSets,
    timestamp_mapper: RtpTimestampMapper,
    expected_sequence_number: Option<u16>,
    current: Option<PartialAccessUnit>,
    fragment: Option<FragmentState>,
    current_frame: Option<PartialFrame>,
    av1_fragment: Option<Av1FragmentState>,
    ready: VecDeque<OwnedEncodedAccessUnit>,
    awaiting_keyframe: bool,
    /// Payload bytes accumulated since the last completed or discarded
    /// access unit, bounding what an endless unit can buffer.
    pending_bytes: usize,
    logged_ssrc_mismatch: bool,
    warned_missing_parameter_sets: bool,
    warned_oversized_pending: bool,
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

/// Packet-loss recovery counters for an [`RtpAccessUnitAssembler`].
#[cfg(test)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RtpDepacketizerStats {
    pub(super) sequence_gaps: u64,
    pub(super) dropped_access_units: u64,
    pub(super) awaiting_keyframe: bool,
}

impl RtpAccessUnitAssembler {
    /// Creates an RTP access-unit assembler for one negotiated video stream.
    pub(super) fn new(
        codec: EncodedVideoCodec,
        payload_type: u8,
        clock_rate: u32,
        parameter_sets: H26xParameterSets,
        resolution: VideoResolution,
    ) -> Result<Self, RtpDepacketizerError> {
        Ok(Self {
            codec,
            payload_type,
            ssrc: None,
            resolution,
            parameter_sets,
            timestamp_mapper: RtpTimestampMapper::new(clock_rate, 0)?,
            expected_sequence_number: None,
            current: None,
            fragment: None,
            current_frame: None,
            av1_fragment: None,
            ready: VecDeque::new(),
            awaiting_keyframe: false,
            pending_bytes: 0,
            logged_ssrc_mismatch: false,
            warned_missing_parameter_sets: false,
            warned_oversized_pending: false,
            sequence_gaps: 0,
            dropped_access_units: 0,
        })
    }

    /// Sets the resolution stamped on access units assembled from now on.
    pub(super) fn set_resolution(&mut self, resolution: VideoResolution) {
        self.resolution = resolution;
    }

    /// Returns the next completed access unit, if any.
    pub(super) fn pop_ready(&mut self) -> Option<OwnedEncodedAccessUnit> {
        self.ready.pop_front()
    }

    /// Returns packet-loss recovery counters.
    #[cfg(test)]
    pub(super) fn stats(&self) -> RtpDepacketizerStats {
        RtpDepacketizerStats {
            sequence_gaps: self.sequence_gaps,
            dropped_access_units: self.dropped_access_units,
            awaiting_keyframe: self.awaiting_keyframe,
        }
    }

    /// Pushes one encoded RTP packet; completed access units become available
    /// through [`Self::pop_ready`].
    pub(super) fn push(&mut self, bytes: &[u8]) -> Result<(), RtpDepacketizerError> {
        let packet = RtpPacket::parse(bytes)?;
        if packet.payload_type != self.payload_type {
            return Ok(());
        }
        match self.ssrc {
            None => self.ssrc = Some(packet.ssrc),
            Some(ssrc) if ssrc != packet.ssrc => {
                if !self.logged_ssrc_mismatch {
                    log::debug!(
                        "dropping RTP packets from unexpected SSRC {:#010x}; \
                         the stream is locked to SSRC {ssrc:#010x}",
                        packet.ssrc,
                    );
                    self.logged_ssrc_mismatch = true;
                }
                return Ok(());
            }
            Some(_) => {}
        }

        self.check_sequence(packet.sequence_number);
        self.finish_on_timestamp_change(packet.timestamp)?;

        match self.codec {
            EncodedVideoCodec::H264 => self.push_h264_payload(&packet)?,
            EncodedVideoCodec::H265 => self.push_h265_payload(&packet)?,
            EncodedVideoCodec::VP8 => self.push_vp8_payload(&packet)?,
            EncodedVideoCodec::VP9 => self.push_vp9_payload(&packet)?,
            EncodedVideoCodec::AV1 => self.push_av1_payload(&packet)?,
        }

        // Bound what an unfinished access unit can buffer: a stream that
        // never completes one must not grow memory without limit. Counting
        // raw payload bytes slightly overestimates, which only trips the
        // generous cap earlier.
        self.pending_bytes = self.pending_bytes.saturating_add(packet.payload.len());
        if self.pending_bytes > MAX_PENDING_ACCESS_UNIT_BYTES {
            if !self.warned_oversized_pending {
                self.warned_oversized_pending = true;
                log::warn!(
                    "discarding an access unit still incomplete after \
                     {MAX_PENDING_ACCESS_UNIT_BYTES} buffered bytes; \
                     waiting for the next keyframe"
                );
            }
            self.discard_in_progress();
            self.dropped_access_units += 1;
            return Ok(());
        }

        if packet.marker {
            if self.fragment.is_some() || self.av1_fragment.is_some() {
                // The marker closed the access unit before the open fragment's
                // end arrived, so its tail packets were lost.
                self.discard_in_progress();
                self.dropped_access_units += 1;
                return Ok(());
            }
            self.finish_pending()?;
        }
        Ok(())
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

    /// Completes the pending access unit when a packet starts a new RTP
    /// timestamp without a marker having closed the previous one.
    ///
    /// Some producers never set the RTP marker bit; on a contiguous sequence,
    /// a timestamp change still proves the previous access unit is complete.
    /// A lost marker packet is a sequence gap instead, which discards the
    /// interrupted access unit before this check runs.
    fn finish_on_timestamp_change(
        &mut self,
        rtp_timestamp: u32,
    ) -> Result<(), RtpDepacketizerError> {
        let pending = self
            .current
            .as_ref()
            .map(|current| current.rtp_timestamp)
            .or_else(|| self.current_frame.as_ref().map(|frame| frame.rtp_timestamp))
            .or_else(|| self.fragment.as_ref().map(|fragment| fragment.rtp_timestamp))
            .or_else(|| self.av1_fragment.as_ref().map(|fragment| fragment.rtp_timestamp));
        let Some(pending) = pending else {
            return Ok(());
        };
        if pending == rtp_timestamp {
            return Ok(());
        }
        if self.fragment.is_some() || self.av1_fragment.is_some() {
            // A new frame began while a fragment was open: its end was lost.
            self.discard_in_progress();
            self.dropped_access_units += 1;
            return Ok(());
        }
        self.finish_pending()
    }

    /// Completes the pending access unit or frame for the source's codec.
    fn finish_pending(&mut self) -> Result<(), RtpDepacketizerError> {
        match self.codec {
            EncodedVideoCodec::H264 | EncodedVideoCodec::H265 => self.finish_current(),
            _ => self.finish_current_frame(),
        }
    }

    /// Discards all partially assembled state and gates output on the next keyframe.
    fn discard_in_progress(&mut self) {
        self.current = None;
        self.fragment = None;
        self.current_frame = None;
        self.av1_fragment = None;
        self.pending_bytes = 0;
        self.awaiting_keyframe = true;
    }

    /// Queues a completed access unit, dropping it while loss recovery gates
    /// output on the next keyframe.
    fn enqueue(&mut self, access_unit: OwnedEncodedAccessUnit) {
        if self.awaiting_keyframe {
            if access_unit.frame_type != EncodedFrameType::Key {
                self.dropped_access_units += 1;
                return;
            }
            self.awaiting_keyframe = false;
        }
        self.ready.push_back(access_unit);
    }

    fn current_mut(
        &mut self,
        rtp_timestamp: u32,
    ) -> Result<&mut PartialAccessUnit, RtpDepacketizerError> {
        if self.current.as_ref().is_some_and(|current| current.rtp_timestamp != rtp_timestamp) {
            // Unreachable after `finish_on_timestamp_change`; kept as a
            // defensive reset.
            self.current = None;
            self.fragment = None;
        }

        if self.current.is_none() {
            let timestamp_us = self.timestamp_mapper.map(rtp_timestamp);
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
            // Unreachable after `finish_on_timestamp_change`; kept as a
            // defensive reset.
            self.current_frame = None;
            self.av1_fragment = None;
        }

        if self.current_frame.is_none() {
            let timestamp_us = self.timestamp_mapper.map(rtp_timestamp);
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

    /// Completes the pending VP8/VP9/AV1 frame and queues it.
    fn finish_current_frame(&mut self) -> Result<(), RtpDepacketizerError> {
        // An open fragment carrying into the next unit undercounts by at
        // most one frame's fragment, which the generous cap absorbs.
        self.pending_bytes = 0;
        let Some(current) = self.current_frame.take() else {
            return Ok(());
        };
        if current.payload.is_empty() {
            return Ok(());
        }

        let access_unit = OwnedEncodedAccessUnit::new(
            self.codec,
            current.payload,
            current.timestamp_us,
            current.frame_type.unwrap_or(EncodedFrameType::Delta),
            self.resolution,
        );
        self.enqueue(access_unit);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn rtp_packet(
        sequence_number: u16,
        timestamp: u32,
        marker: bool,
        payload: &[u8],
    ) -> Vec<u8> {
        rtp_packet_from(sequence_number, timestamp, marker, 96, 0x1122_3344, payload)
    }

    pub(super) fn rtp_packet_from(
        sequence_number: u16,
        timestamp: u32,
        marker: bool,
        payload_type: u8,
        ssrc: u32,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut packet = Vec::with_capacity(12 + payload.len());
        packet.push(0x80);
        packet.push(if marker { 0x80 | payload_type } else { payload_type });
        packet.extend_from_slice(&sequence_number.to_be_bytes());
        packet.extend_from_slice(&timestamp.to_be_bytes());
        packet.extend_from_slice(&ssrc.to_be_bytes());
        packet.extend_from_slice(payload);
        packet
    }

    pub(super) fn assembler(codec: EncodedVideoCodec) -> RtpAccessUnitAssembler {
        RtpAccessUnitAssembler::new(
            codec,
            96,
            90_000,
            H26xParameterSets::default(),
            VideoResolution::new(640, 480),
        )
        .unwrap()
    }

    pub(super) fn push_one(
        assembler: &mut RtpAccessUnitAssembler,
        bytes: &[u8],
    ) -> Option<OwnedEncodedAccessUnit> {
        assembler.push(bytes).unwrap();
        assembler.pop_ready()
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
        let mut mapper = RtpTimestampMapper::new(90_000, 1_000).unwrap();
        assert_eq!(mapper.map(u32::MAX - 89), 1_000);
        assert_eq!(mapper.map(0), 2_000);
    }

    #[test]
    fn maps_rtp_timestamps_across_multiple_rollovers() {
        let mut mapper = RtpTimestampMapper::new(90_000, 0).unwrap();
        let step = 1u32 << 30;
        let mut rtp_timestamp = 0u32;
        let mut last_us = mapper.map(rtp_timestamp);
        for _ in 0..20 {
            rtp_timestamp = rtp_timestamp.wrapping_add(step);
            let mapped_us = mapper.map(rtp_timestamp);
            assert!(mapped_us > last_us, "mapped timestamps must stay monotonic");
            last_us = mapped_us;
        }
        assert_eq!(last_us, (20i64 << 30) * 1_000_000 / 90_000);
    }

    #[test]
    fn maps_reordered_rtp_timestamps() {
        let mut mapper = RtpTimestampMapper::new(90_000, 1_000).unwrap();
        assert_eq!(mapper.map(9_000), 1_000);
        assert_eq!(mapper.map(18_000), 101_000);
        // A late packet maps behind the stream without disturbing what follows.
        assert_eq!(mapper.map(15_000), 67_666);
        assert_eq!(mapper.map(27_000), 201_000);
    }

    #[test]
    fn rejects_zero_clock_rate() {
        assert_eq!(
            RtpTimestampMapper::new(0, 0).unwrap_err(),
            RtpDepacketizerError::InvalidClockRate
        );
    }

    #[test]
    fn ignores_other_payload_types() {
        let mut assembler = assembler(EncodedVideoCodec::H264);
        // An interloping payload type must not count as a sequence gap.
        let other = rtp_packet_from(50, 12_000, true, 97, 0x1122_3344, &[0x65, 9]);
        let first = rtp_packet(10, 12_000, false, &[0x65, 1, 2]);
        let second = rtp_packet(11, 12_000, true, &[0x41, 3]);

        assert!(push_one(&mut assembler, &first).is_none());
        assert!(push_one(&mut assembler, &other).is_none());
        let access_unit = push_one(&mut assembler, &second).unwrap();
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2, 0, 0, 0, 1, 0x41, 3]);
        assert_eq!(assembler.stats().sequence_gaps, 0);
    }

    #[test]
    fn locks_to_first_ssrc() {
        let mut assembler = assembler(EncodedVideoCodec::H264);
        let first = rtp_packet_from(10, 12_000, true, 96, 0xaaaa_aaaa, &[0x65, 1]);
        let intruder = rtp_packet_from(70, 15_000, true, 96, 0xbbbb_bbbb, &[0x65, 2]);
        let second = rtp_packet_from(11, 15_000, true, 96, 0xaaaa_aaaa, &[0x41, 3]);

        assert!(push_one(&mut assembler, &first).unwrap().frame_type == EncodedFrameType::Key);
        assert!(push_one(&mut assembler, &intruder).is_none());
        let access_unit = push_one(&mut assembler, &second).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Delta);
        assert_eq!(assembler.stats().sequence_gaps, 0);
    }

    #[test]
    fn caps_h264_pending_access_unit_bytes() {
        let mut assembler = assembler(EncodedVideoCodec::H264);
        let chunk = [0xaa; 60_000];

        // An FU-A start followed by endless continuations at one timestamp:
        // the fragment never completes, so buffering must stop at the cap.
        let mut payload = vec![0x7c, 0x85];
        payload.extend_from_slice(&chunk);
        assert!(push_one(&mut assembler, &rtp_packet(0, 12_000, false, &payload)).is_none());

        let mut payload = vec![0x7c, 0x05];
        payload.extend_from_slice(&chunk);
        let mut sequence_number = 1u16;
        while !assembler.stats().awaiting_keyframe {
            assert!(sequence_number < 1_000, "the pending byte cap never triggered");
            let packet = rtp_packet(sequence_number, 12_000, false, &payload);
            assert!(push_one(&mut assembler, &packet).is_none());
            sequence_number += 1;
        }
        assert!(assembler.stats().dropped_access_units >= 1);

        // The stream recovers at the next keyframe.
        let key = rtp_packet(sequence_number, 15_000, true, &[0x65, 1, 2]);
        let access_unit = push_one(&mut assembler, &key).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
    }

    #[test]
    fn caps_vp8_pending_frame_bytes() {
        let mut assembler = assembler(EncodedVideoCodec::VP8);
        let chunk = [0xaa; 60_000];

        let mut payload = vec![0x10, 0x00];
        payload.extend_from_slice(&chunk);
        assert!(push_one(&mut assembler, &rtp_packet(0, 12_000, false, &payload)).is_none());

        let mut payload = vec![0x00];
        payload.extend_from_slice(&chunk);
        let mut sequence_number = 1u16;
        while !assembler.stats().awaiting_keyframe {
            assert!(sequence_number < 1_000, "the pending byte cap never triggered");
            let packet = rtp_packet(sequence_number, 12_000, false, &payload);
            assert!(push_one(&mut assembler, &packet).is_none());
            sequence_number += 1;
        }

        let key = rtp_packet(sequence_number, 15_000, true, &[0x10, 0x00, 1, 2]);
        let access_unit = push_one(&mut assembler, &key).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
    }

    #[test]
    fn timestamp_change_completes_marker_less_access_unit() {
        let mut assembler = assembler(EncodedVideoCodec::H264);
        // The producer never sets the marker bit.
        let first = rtp_packet(10, 12_000, false, &[0x65, 1, 2]);
        let second = rtp_packet(11, 15_000, false, &[0x41, 3, 4]);
        let third = rtp_packet(12, 18_000, false, &[0x41, 5, 6]);

        assert!(push_one(&mut assembler, &first).is_none());
        let key = push_one(&mut assembler, &second).unwrap();
        assert_eq!(key.frame_type, EncodedFrameType::Key);
        assert_eq!(key.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);

        let delta = push_one(&mut assembler, &third).unwrap();
        assert_eq!(delta.frame_type, EncodedFrameType::Delta);
        assert_eq!(delta.payload.as_ref(), &[0, 0, 0, 1, 0x41, 3, 4]);
    }

    #[test]
    fn timestamp_change_with_open_fragment_drops_access_unit() {
        let mut assembler = assembler(EncodedVideoCodec::H264);
        let start = rtp_packet(10, 12_000, false, &[0x7c, 0x85, 1, 2]);
        // The fragment end never arrives; the next frame starts instead.
        let next = rtp_packet(11, 15_000, true, &[0x65, 3, 4]);

        assert!(push_one(&mut assembler, &start).is_none());
        let access_unit = push_one(&mut assembler, &next).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(assembler.stats().dropped_access_units, 1);
    }
}
