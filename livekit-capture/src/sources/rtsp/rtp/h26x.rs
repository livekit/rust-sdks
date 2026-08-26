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

//! H.264 (RFC 6184) and H.265 (RFC 7798) RTP payload handling.

use super::{FragmentState, RtpAccessUnitAssembler, RtpDepacketizerError, RtpPacket};
use crate::encoded::{
    h26x::{access_unit_from_nalus, h264_nal_type, h265_nal_type},
    EncodedFrameType, EncodedVideoCodec,
};

impl RtpAccessUnitAssembler {
    pub(super) fn push_h264_payload(
        &mut self,
        packet: &RtpPacket<'_>,
    ) -> Result<(), RtpDepacketizerError> {
        let payload = packet.payload;
        let Some(&header) = payload.first() else {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        };
        let nal_type = header & 0x1f;

        match nal_type {
            1..=23 => self.current_mut(packet.timestamp)?.nal_units.push(payload.to_vec()),
            24 => self.push_h26x_aggregation(packet.timestamp, &payload[1..])?,
            28 => self.push_h264_fu_a(packet.timestamp, payload)?,
            _ => return Err(RtpDepacketizerError::UnsupportedPayload),
        }

        Ok(())
    }

    pub(super) fn push_h265_payload(
        &mut self,
        packet: &RtpPacket<'_>,
    ) -> Result<(), RtpDepacketizerError> {
        let payload = packet.payload;
        if payload.len() < 2 {
            return Err(RtpDepacketizerError::UnsupportedPayload);
        }
        let nal_type = (payload[0] >> 1) & 0x3f;

        match nal_type {
            0..=47 => self.current_mut(packet.timestamp)?.nal_units.push(payload.to_vec()),
            48 => self.push_h26x_aggregation(packet.timestamp, &payload[2..])?,
            49 => self.push_h265_fragment(packet.timestamp, payload)?,
            _ => return Err(RtpDepacketizerError::UnsupportedPayload),
        }

        Ok(())
    }

    /// Unpacks the length-prefixed NAL units of an H.264 STAP-A or H.265 AP
    /// payload, whose aggregation headers the caller has already stripped.
    fn push_h26x_aggregation(
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

    /// Completes the pending H.26x access unit and queues it.
    ///
    /// Keyframe access units missing parameter sets receive the ones the SDP
    /// carried out-of-band, so every published keyframe is self-contained:
    /// passthrough subscribers may join mid-stream and can only initialize
    /// their decoder from parameter sets inside the keyframe itself.
    pub(super) fn finish_current(&mut self) -> Result<(), RtpDepacketizerError> {
        let Some(current) = self.current.take() else {
            return Ok(());
        };
        if current.nal_units.is_empty() {
            return Ok(());
        }

        let mut presence = NalPresence::default();
        for nal in &current.nal_units {
            presence.record(self.codec, nal);
        }

        // Prepend only the missing parameter-set kinds, in VPS, SPS, PPS
        // order; in-band parameter sets pass through untouched.
        let mut nal_units = Vec::with_capacity(current.nal_units.len() + 3);
        if presence.idr {
            if self.codec == EncodedVideoCodec::H265 && !presence.vps {
                nal_units.extend(self.parameter_sets.vps.iter().map(Vec::as_slice));
            }
            if !presence.sps {
                nal_units.extend(self.parameter_sets.sps.iter().map(Vec::as_slice));
            }
            if !presence.pps {
                nal_units.extend(self.parameter_sets.pps.iter().map(Vec::as_slice));
            }
        }
        nal_units.extend(current.nal_units.iter().map(Vec::as_slice));

        let access_unit =
            access_unit_from_nalus(self.codec, &nal_units, current.timestamp_us, self.resolution)?;

        if presence.idr
            && access_unit.frame_type != EncodedFrameType::Key
            && !self.warned_missing_parameter_sets
        {
            self.warned_missing_parameter_sets = true;
            log::warn!(
                "H.265 keyframe lacks VPS/SPS/PPS and the SDP provided none; \
                 the stream cannot be published until a self-contained keyframe arrives"
            );
        }

        self.enqueue(access_unit);
        Ok(())
    }
}

/// Which access-unit-defining NAL kinds appear in a pending access unit.
#[derive(Debug, Clone, Copy, Default)]
struct NalPresence {
    vps: bool,
    sps: bool,
    pps: bool,
    idr: bool,
}

impl NalPresence {
    fn record(&mut self, codec: EncodedVideoCodec, nal: &[u8]) {
        match codec {
            EncodedVideoCodec::H264 => match h264_nal_type(nal) {
                Ok(5) => self.idr = true,
                Ok(7) => self.sps = true,
                Ok(8) => self.pps = true,
                _ => {}
            },
            EncodedVideoCodec::H265 => match h265_nal_type(nal) {
                Ok(19 | 20) => self.idr = true,
                Ok(32) => self.vps = true,
                Ok(33) => self.sps = true,
                Ok(34) => self.pps = true,
                _ => {}
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{assembler, push_one, rtp_packet};
    use super::*;
    use crate::{
        encoded::OwnedEncodedAccessUnit, primitive::VideoResolution,
        sources::rtsp::rtp::H26xParameterSets,
    };

    fn assembler_with_parameter_sets(
        codec: EncodedVideoCodec,
        parameter_sets: H26xParameterSets,
    ) -> RtpAccessUnitAssembler {
        RtpAccessUnitAssembler::new(
            codec,
            96,
            90_000,
            parameter_sets,
            VideoResolution::new(640, 480),
        )
        .unwrap()
    }

    fn annex_b_nals(access_unit: &OwnedEncodedAccessUnit) -> Vec<&[u8]> {
        crate::encoded::h26x::annex_b_nalus(&access_unit.payload)
    }

    #[test]
    fn assembles_h264_single_nal_access_unit() {
        let mut assembler = assembler(EncodedVideoCodec::H264);
        let packet = rtp_packet(10, 12_000, true, &[0x65, 1, 2]);

        let access_unit = push_one(&mut assembler, &packet).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);
    }

    #[test]
    fn assembles_h264_stap_a() {
        let mut assembler = assembler(EncodedVideoCodec::H264);
        // STAP-A carrying SPS (2 bytes) and PPS (2 bytes), then an IDR.
        let stap = rtp_packet(10, 12_000, false, &[0x18, 0, 2, 0x67, 9, 0, 2, 0x68, 8]);
        let idr = rtp_packet(11, 12_000, true, &[0x65, 1]);

        assert!(push_one(&mut assembler, &stap).is_none());
        let access_unit = push_one(&mut assembler, &idr).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(
            access_unit.payload.as_ref(),
            &[0, 0, 0, 1, 0x67, 9, 0, 0, 0, 1, 0x68, 8, 0, 0, 0, 1, 0x65, 1]
        );
    }

    #[test]
    fn assembles_h264_fu_a() {
        let mut assembler = assembler(EncodedVideoCodec::H264);
        let start = rtp_packet(10, 12_000, false, &[0x7c, 0x85, 1, 2]);
        let end = rtp_packet(11, 12_000, true, &[0x7c, 0x45, 3, 4]);

        assert!(push_one(&mut assembler, &start).is_none());
        let access_unit = push_one(&mut assembler, &end).unwrap();
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2, 3, 4]);
    }

    #[test]
    fn sequence_gap_recovers_h264_at_next_keyframe() {
        let mut assembler = assembler(EncodedVideoCodec::H264);
        let start = rtp_packet(10, 12_000, false, &[0x7c, 0x85, 1, 2]);
        let delta = rtp_packet(12, 15_000, true, &[0x41, 1, 2]);
        let key = rtp_packet(13, 18_000, true, &[0x65, 3, 4]);

        assert!(push_one(&mut assembler, &start).is_none());
        // The gap dropped the fragment; the delta frame after it is withheld.
        assert!(push_one(&mut assembler, &delta).is_none());
        let stats = assembler.stats();
        assert_eq!(stats.sequence_gaps, 1);
        assert_eq!(stats.dropped_access_units, 1);
        assert!(stats.awaiting_keyframe);

        let access_unit = push_one(&mut assembler, &key).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 3, 4]);
        let stats = assembler.stats();
        assert_eq!(stats.dropped_access_units, 1);
        assert!(!stats.awaiting_keyframe);
    }

    #[test]
    fn marker_with_open_h264_fragment_drops_access_unit() {
        let mut assembler = assembler(EncodedVideoCodec::H264);
        let start = rtp_packet(10, 12_000, false, &[0x7c, 0x85, 1, 2]);
        let truncated = rtp_packet(11, 12_000, true, &[0x7c, 0x05, 3, 4]);
        let key = rtp_packet(12, 15_000, true, &[0x65, 5, 6]);

        assert!(push_one(&mut assembler, &start).is_none());
        // The marker arrived without the FU end bit: the fragment is truncated.
        assert!(push_one(&mut assembler, &truncated).is_none());
        let stats = assembler.stats();
        assert_eq!(stats.sequence_gaps, 0);
        assert_eq!(stats.dropped_access_units, 1);
        assert!(stats.awaiting_keyframe);

        let access_unit = push_one(&mut assembler, &key).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 5, 6]);
        assert!(!assembler.stats().awaiting_keyframe);
    }

    #[test]
    fn drops_h264_fu_continuation_without_start() {
        let mut assembler = assembler(EncodedVideoCodec::H264);
        let continuation = rtp_packet(10, 12_000, false, &[0x7c, 0x05, 1, 2]);
        let key = rtp_packet(11, 15_000, true, &[0x65, 3, 4]);

        assert!(push_one(&mut assembler, &continuation).is_none());
        assert!(assembler.stats().awaiting_keyframe);

        let access_unit = push_one(&mut assembler, &key).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 3, 4]);
    }

    #[test]
    fn assembles_h265_fragment_units() {
        let mut assembler = assembler(EncodedVideoCodec::H265);
        // FU (type 49) carrying an IDR_W_RADL (type 19) split in two, after
        // an AP (type 48) carrying VPS, SPS, and PPS.
        let parameter_sets = rtp_packet(
            9,
            12_000,
            false,
            &[
                0x60, 0x01, // AP NAL header.
                0, 2, 0x40, 0x01, // VPS (type 32).
                0, 2, 0x42, 0x01, // SPS (type 33).
                0, 2, 0x44, 0x01, // PPS (type 34).
            ],
        );
        let start = rtp_packet(10, 12_000, false, &[0x62, 0x01, 0x93, 1, 2]);
        let end = rtp_packet(11, 12_000, true, &[0x62, 0x01, 0x53, 3, 4]);

        assert!(push_one(&mut assembler, &parameter_sets).is_none());
        assert!(push_one(&mut assembler, &start).is_none());
        let access_unit = push_one(&mut assembler, &end).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        let nals = annex_b_nals(&access_unit);
        assert_eq!(nals.len(), 4);
        assert_eq!(nals[3], &[0x26, 0x01, 1, 2, 3, 4]);
    }

    #[test]
    fn injects_sdp_parameter_sets_into_h264_keyframe() {
        let parameter_sets = H26xParameterSets {
            vps: Vec::new(),
            sps: vec![vec![0x67, 9, 8]],
            pps: vec![vec![0x68, 7]],
        };
        let mut assembler =
            assembler_with_parameter_sets(EncodedVideoCodec::H264, parameter_sets);
        let idr = rtp_packet(10, 12_000, true, &[0x65, 1, 2]);
        let delta = rtp_packet(11, 15_000, true, &[0x41, 3]);

        let access_unit = push_one(&mut assembler, &idr).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(
            access_unit.payload.as_ref(),
            &[0, 0, 0, 1, 0x67, 9, 8, 0, 0, 0, 1, 0x68, 7, 0, 0, 0, 1, 0x65, 1, 2]
        );

        // Delta frames pass through untouched.
        let access_unit = push_one(&mut assembler, &delta).unwrap();
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x41, 3]);
    }

    #[test]
    fn injects_sdp_parameter_sets_into_h265_keyframe() {
        let parameter_sets = H26xParameterSets {
            vps: vec![vec![0x40, 0x01, 1]],
            sps: vec![vec![0x42, 0x01, 2]],
            pps: vec![vec![0x44, 0x01, 3]],
        };
        let mut assembler =
            assembler_with_parameter_sets(EncodedVideoCodec::H265, parameter_sets);
        // An IDR-only access unit classifies as a keyframe only once the SDP
        // parameter sets are injected.
        let idr = rtp_packet(10, 12_000, true, &[0x26, 0x01, 1, 2]);

        let access_unit = push_one(&mut assembler, &idr).unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        let nals = annex_b_nals(&access_unit);
        assert_eq!(nals, vec![
            &[0x40, 0x01, 1][..],
            &[0x42, 0x01, 2][..],
            &[0x44, 0x01, 3][..],
            &[0x26, 0x01, 1, 2][..],
        ]);
    }

    #[test]
    fn does_not_duplicate_in_band_parameter_sets() {
        let parameter_sets = H26xParameterSets {
            vps: Vec::new(),
            sps: vec![vec![0x67, 99]],
            pps: vec![vec![0x68, 99]],
        };
        let mut assembler =
            assembler_with_parameter_sets(EncodedVideoCodec::H264, parameter_sets);
        // The stream repeats its own parameter sets in-band.
        let stap = rtp_packet(10, 12_000, false, &[0x18, 0, 2, 0x67, 1, 0, 2, 0x68, 2]);
        let idr = rtp_packet(11, 12_000, true, &[0x65, 3]);

        assert!(push_one(&mut assembler, &stap).is_none());
        let access_unit = push_one(&mut assembler, &idr).unwrap();
        assert_eq!(
            access_unit.payload.as_ref(),
            &[0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x68, 2, 0, 0, 0, 1, 0x65, 3]
        );
    }
}
