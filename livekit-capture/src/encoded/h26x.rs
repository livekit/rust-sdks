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

use std::ops::Range;

use bytes::Bytes;

use crate::{
    encoded::{
        annex_b_payload, h264_nal_type, h265_nal_type, CodecSpecific, EncodedFrameType,
        EncodedVideoCodec, H264PacketizationMode, OwnedEncodedAccessUnit,
    },
    error::CaptureError,
};

/// H26x Annex-B parser state.
#[derive(Debug, Clone)]
pub struct AnnexBAccessUnitParser {
    codec: EncodedVideoCodec,
    pending: Vec<u8>,
    next_timestamp_us: i64,
    frame_interval_us: i64,
    width: u32,
    height: u32,
}

/// H.264/AVC length-prefixed parser state.
#[cfg(any(feature = "tcpsink", test))]
#[derive(Debug, Clone)]
pub(crate) struct AvcAccessUnitParser {
    pending: Vec<u8>,
    nal_length_size: u8,
    next_timestamp_us: i64,
    frame_interval_us: i64,
    width: u32,
    height: u32,
}

impl AnnexBAccessUnitParser {
    /// Creates a parser for H.264 or H.265 Annex-B byte streams.
    pub fn new(
        codec: EncodedVideoCodec,
        start_timestamp_us: i64,
        frame_interval_us: i64,
        width: u32,
        height: u32,
    ) -> Result<Self, CaptureError> {
        match codec {
            EncodedVideoCodec::H264 | EncodedVideoCodec::H265 => {}
            EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
                return Err(CaptureError::UnsupportedCodec(codec));
            }
        }

        Ok(Self {
            codec,
            pending: Vec::new(),
            next_timestamp_us: start_timestamp_us,
            frame_interval_us,
            width,
            height,
        })
    }

    /// Pushes encoded bytes and returns the next complete access unit if one is found.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Option<OwnedEncodedAccessUnit>, CaptureError> {
        self.pending.extend_from_slice(bytes);
        self.drain_next(false)
    }

    /// Flushes the pending bytes as the final access unit.
    pub fn flush(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, CaptureError> {
        self.drain_next(true)
    }

    fn drain_next(&mut self, at_eof: bool) -> Result<Option<OwnedEncodedAccessUnit>, CaptureError> {
        let ranges = annex_b_nal_ranges(&self.pending);
        if ranges.is_empty() {
            return Ok(None);
        }

        let Some(split_at) = access_unit_split_index(self.codec, &self.pending, &ranges)? else {
            if at_eof {
                return self.take_access_unit(self.pending.len());
            }
            return Ok(None);
        };

        self.take_access_unit(split_at)
    }

    fn take_access_unit(
        &mut self,
        byte_len: usize,
    ) -> Result<Option<OwnedEncodedAccessUnit>, CaptureError> {
        if byte_len == 0 {
            return Ok(None);
        }

        let access_unit = self.pending[..byte_len].to_vec();
        self.pending.drain(..byte_len);
        let timestamp_us = self.next_timestamp_us;
        self.next_timestamp_us = self.next_timestamp_us.saturating_add(self.frame_interval_us);
        access_unit_from_annex_b(
            self.codec,
            Bytes::from(access_unit),
            timestamp_us,
            self.width,
            self.height,
        )
        .map(Some)
    }
}

#[cfg(any(feature = "tcpsink", test))]
impl AvcAccessUnitParser {
    /// Creates a parser for H.264/AVC length-prefixed byte streams.
    pub(crate) fn new(
        nal_length_size: u8,
        start_timestamp_us: i64,
        frame_interval_us: i64,
        width: u32,
        height: u32,
    ) -> Result<Self, CaptureError> {
        validate_avc_nal_length_size(nal_length_size)?;

        Ok(Self {
            pending: Vec::new(),
            nal_length_size,
            next_timestamp_us: start_timestamp_us,
            frame_interval_us,
            width,
            height,
        })
    }

    /// Pushes encoded bytes and returns the next complete access unit if one is found.
    pub(crate) fn push(
        &mut self,
        bytes: &[u8],
    ) -> Result<Option<OwnedEncodedAccessUnit>, CaptureError> {
        self.pending.extend_from_slice(bytes);
        self.drain_next(false)
    }

    /// Flushes the pending bytes as the final access unit.
    pub(crate) fn flush(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, CaptureError> {
        self.drain_next(true)
    }

    fn drain_next(&mut self, at_eof: bool) -> Result<Option<OwnedEncodedAccessUnit>, CaptureError> {
        let ranges = avc_nal_ranges(&self.pending, self.nal_length_size, at_eof)?;
        if ranges.is_empty() {
            return Ok(None);
        }

        let Some(split_at) =
            avc_access_unit_split_index(&self.pending, &ranges, self.nal_length_size as usize)?
        else {
            if at_eof {
                return self.take_access_unit(self.pending.len());
            }
            return Ok(None);
        };

        self.take_access_unit(split_at)
    }

    fn take_access_unit(
        &mut self,
        byte_len: usize,
    ) -> Result<Option<OwnedEncodedAccessUnit>, CaptureError> {
        if byte_len == 0 {
            return Ok(None);
        }

        let access_unit = self.pending[..byte_len].to_vec();
        self.pending.drain(..byte_len);
        let timestamp_us = self.next_timestamp_us;
        self.next_timestamp_us = self.next_timestamp_us.saturating_add(self.frame_interval_us);
        access_unit_from_h264_avc(
            &access_unit,
            self.nal_length_size,
            timestamp_us,
            self.width,
            self.height,
        )
        .map(Some)
    }
}

/// Returns NAL-unit byte ranges for an Annex-B access unit or stream chunk.
pub fn annex_b_nal_ranges(bytes: &[u8]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut cursor = 0;
    let mut current_start = None;

    while let Some((prefix_start, prefix_len)) = find_start_code(&bytes[cursor..]) {
        let prefix_start = cursor + prefix_start;
        let nal_start = prefix_start + prefix_len;
        if let Some(start) = current_start.replace(nal_start) {
            if start < prefix_start {
                ranges.push(start..prefix_start);
            }
        }
        cursor = nal_start;
    }

    if let Some(start) = current_start {
        if start < bytes.len() {
            ranges.push(start..bytes.len());
        }
    }

    ranges
}

/// Returns borrowed NAL units from an Annex-B buffer.
pub fn annex_b_nalus(bytes: &[u8]) -> Result<Vec<&[u8]>, CaptureError> {
    let nals = annex_b_nal_ranges(bytes)
        .into_iter()
        .map(|range| &bytes[range])
        .filter(|nal| !nal.is_empty())
        .collect::<Vec<_>>();
    Ok(nals)
}

/// Creates an Annex-B access unit from H.264/AVC length-prefixed NAL units.
pub(crate) fn access_unit_from_h264_avc(
    payload: &[u8],
    nal_length_size: u8,
    timestamp_us: i64,
    width: u32,
    height: u32,
) -> Result<OwnedEncodedAccessUnit, CaptureError> {
    let nals = avc_nalus(payload, nal_length_size)?;
    access_unit_from_nalus(EncodedVideoCodec::H264, &nals, timestamp_us, width, height)
}

/// Creates an access unit from an Annex-B buffer.
pub fn access_unit_from_annex_b(
    codec: EncodedVideoCodec,
    payload: Bytes,
    timestamp_us: i64,
    width: u32,
    height: u32,
) -> Result<OwnedEncodedAccessUnit, CaptureError> {
    if payload.is_empty() {
        return Err(CaptureError::EmptyPayload);
    }

    let frame_type = if is_keyframe_annex_b(codec, &payload)? {
        EncodedFrameType::Key
    } else {
        EncodedFrameType::Delta
    };
    let mut access_unit =
        OwnedEncodedAccessUnit::new(codec, payload, timestamp_us, frame_type, width, height);
    access_unit.codec_specific = match codec {
        EncodedVideoCodec::H264 => {
            CodecSpecific::H264 { packetization_mode: H264PacketizationMode::NonInterleaved }
        }
        EncodedVideoCodec::H265 => CodecSpecific::H265,
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
            return Err(CaptureError::UnsupportedCodec(codec));
        }
    };
    Ok(access_unit)
}

/// Creates an Annex-B access unit from raw NAL units.
pub fn access_unit_from_nalus(
    codec: EncodedVideoCodec,
    nal_units: &[&[u8]],
    timestamp_us: i64,
    width: u32,
    height: u32,
) -> Result<OwnedEncodedAccessUnit, CaptureError> {
    let payload = Bytes::from(annex_b_payload(nal_units)?);
    access_unit_from_annex_b(codec, payload, timestamp_us, width, height)
}

/// Returns true when an Annex-B access unit contains an intra/key picture.
pub fn is_keyframe_annex_b(codec: EncodedVideoCodec, bytes: &[u8]) -> Result<bool, CaptureError> {
    let nals = annex_b_nalus(bytes)?;
    match codec {
        EncodedVideoCodec::H264 => {
            nals.iter().try_fold(false, |is_key, nal| Ok(is_key || h264_nal_type(nal)? == 5))
        }
        EncodedVideoCodec::H265 => nals.iter().try_fold(false, |is_key, nal| {
            let nal_type = h265_nal_type(nal)?;
            Ok(is_key || (16..=21).contains(&nal_type))
        }),
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
            Err(CaptureError::UnsupportedCodec(codec))
        }
    }
}

fn access_unit_split_index(
    codec: EncodedVideoCodec,
    bytes: &[u8],
    ranges: &[Range<usize>],
) -> Result<Option<usize>, CaptureError> {
    if ranges.len() < 2 {
        return Ok(None);
    }

    let first_nal = &bytes[ranges[0].clone()];
    let mut seen_vcl = is_vcl_nal(codec, first_nal)?;
    for range in ranges.iter().skip(1) {
        let nal = &bytes[range.clone()];
        if is_access_unit_delimiter(codec, nal)? && seen_vcl {
            return split_start_code_index(bytes, range.start).map(Some);
        }
        seen_vcl |= is_vcl_nal(codec, nal)?;
    }
    Ok(None)
}

#[cfg(any(feature = "tcpsink", test))]
fn avc_access_unit_split_index(
    bytes: &[u8],
    ranges: &[Range<usize>],
    nal_length_size: usize,
) -> Result<Option<usize>, CaptureError> {
    if ranges.len() < 2 {
        return Ok(None);
    }

    let first_nal = &bytes[ranges[0].clone()];
    let mut seen_vcl = is_vcl_nal(EncodedVideoCodec::H264, first_nal)?;
    for range in ranges.iter().skip(1) {
        let nal = &bytes[range.clone()];
        if is_access_unit_delimiter(EncodedVideoCodec::H264, nal)? && seen_vcl {
            return range
                .start
                .checked_sub(nal_length_size)
                .ok_or(CaptureError::InvalidEncodedData("missing AVC NAL length"))
                .map(Some);
        }
        seen_vcl |= is_vcl_nal(EncodedVideoCodec::H264, nal)?;
    }
    Ok(None)
}

fn split_start_code_index(bytes: &[u8], nal_start: usize) -> Result<usize, CaptureError> {
    if nal_start >= 4 && bytes[nal_start - 4..nal_start] == [0, 0, 0, 1] {
        return Ok(nal_start - 4);
    }
    if nal_start >= 3 && bytes[nal_start - 3..nal_start] == [0, 0, 1] {
        return Ok(nal_start - 3);
    }
    Err(CaptureError::InvalidEncodedData("missing Annex-B start code"))
}

fn is_access_unit_delimiter(codec: EncodedVideoCodec, nal: &[u8]) -> Result<bool, CaptureError> {
    Ok(match codec {
        EncodedVideoCodec::H264 => h264_nal_type(nal)? == 9,
        EncodedVideoCodec::H265 => h265_nal_type(nal)? == 35,
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
            return Err(CaptureError::UnsupportedCodec(codec));
        }
    })
}

fn is_vcl_nal(codec: EncodedVideoCodec, nal: &[u8]) -> Result<bool, CaptureError> {
    Ok(match codec {
        EncodedVideoCodec::H264 => (1..=5).contains(&h264_nal_type(nal)?),
        EncodedVideoCodec::H265 => h265_nal_type(nal)? <= 31,
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
            return Err(CaptureError::UnsupportedCodec(codec));
        }
    })
}

fn find_start_code(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut idx = 0;
    while idx + 3 <= bytes.len() {
        if bytes[idx..].starts_with(&[0, 0, 1]) {
            return Some((idx, 3));
        }
        if idx + 4 <= bytes.len() && bytes[idx..].starts_with(&[0, 0, 0, 1]) {
            return Some((idx, 4));
        }
        idx += 1;
    }
    None
}

fn avc_nalus(payload: &[u8], nal_length_size: u8) -> Result<Vec<&[u8]>, CaptureError> {
    let ranges = avc_nal_ranges(payload, nal_length_size, true)?;
    if ranges.is_empty() {
        return Err(CaptureError::EmptyPayload);
    }
    Ok(ranges.into_iter().map(|range| &payload[range]).collect())
}

fn avc_nal_ranges(
    bytes: &[u8],
    nal_length_size: u8,
    at_eof: bool,
) -> Result<Vec<Range<usize>>, CaptureError> {
    validate_avc_nal_length_size(nal_length_size)?;

    let nal_length_size = nal_length_size as usize;
    let mut ranges = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes.len() - cursor < nal_length_size {
            if at_eof {
                return Err(CaptureError::InvalidEncodedData("truncated AVC NAL length"));
            }
            break;
        }

        let nal_len = read_avc_nal_length(&bytes[cursor..cursor + nal_length_size]);
        cursor += nal_length_size;
        if nal_len == 0 {
            return Err(CaptureError::InvalidEncodedData("empty AVC NAL unit"));
        }

        let Some(nal_end) = cursor.checked_add(nal_len) else {
            return Err(CaptureError::InvalidEncodedData("AVC NAL unit length overflow"));
        };
        if nal_end > bytes.len() {
            if at_eof {
                return Err(CaptureError::InvalidEncodedData("truncated AVC NAL unit"));
            }
            break;
        }

        ranges.push(cursor..nal_end);
        cursor = nal_end;
    }

    Ok(ranges)
}

fn read_avc_nal_length(bytes: &[u8]) -> usize {
    bytes.iter().fold(0usize, |len, byte| (len << 8) | usize::from(*byte))
}

fn validate_avc_nal_length_size(nal_length_size: u8) -> Result<(), CaptureError> {
    if (1..=4).contains(&nal_length_size) {
        return Ok(());
    }
    Err(CaptureError::InvalidEncodedData("invalid AVC NAL length size"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_annex_b_nals_with_three_and_four_byte_prefixes() {
        let bytes = [0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x65, 2, 3];
        let nals = annex_b_nalus(&bytes).unwrap();
        assert_eq!(nals, vec![&[0x67, 1][..], &[0x65, 2, 3][..]]);
    }

    #[test]
    fn detects_h264_keyframe_from_annex_b() {
        let bytes = [0, 0, 0, 1, 0x61, 1, 0, 0, 0, 1, 0x65, 2];
        assert!(is_keyframe_annex_b(EncodedVideoCodec::H264, &bytes).unwrap());
    }

    #[test]
    fn access_unit_from_avc_converts_length_prefixed_nals() {
        let bytes = [0, 0, 0, 4, 0x67, 1, 2, 3, 0, 0, 0, 3, 0x65, 4, 5];
        let au = access_unit_from_h264_avc(&bytes, 4, 10, 640, 480).unwrap();

        assert_eq!(au.codec, EncodedVideoCodec::H264);
        assert_eq!(au.frame_type, EncodedFrameType::Key);
        assert_eq!(au.payload.as_ref(), &[0, 0, 0, 1, 0x67, 1, 2, 3, 0, 0, 0, 1, 0x65, 4, 5]);
    }

    #[test]
    fn access_unit_from_avc_supports_two_byte_lengths() {
        let bytes = [0, 2, 0x61, 1];
        let au = access_unit_from_h264_avc(&bytes, 2, 10, 640, 480).unwrap();

        assert_eq!(au.frame_type, EncodedFrameType::Delta);
        assert_eq!(au.payload.as_ref(), &[0, 0, 0, 1, 0x61, 1]);
    }

    #[test]
    fn access_unit_from_avc_rejects_truncated_nal() {
        let err = access_unit_from_h264_avc(&[0, 0, 0, 3, 0x65], 4, 10, 640, 480).unwrap_err();

        assert_eq!(err, CaptureError::InvalidEncodedData("truncated AVC NAL unit"));
    }

    #[test]
    fn parser_flushes_final_access_unit() {
        let mut parser =
            AnnexBAccessUnitParser::new(EncodedVideoCodec::H264, 100, 33_333, 640, 480).unwrap();
        assert!(parser.push(&[0, 0, 1, 0x65, 1, 2]).unwrap().is_none());
        let au = parser.flush().unwrap().unwrap();
        assert_eq!(au.timestamp_us, 100);
        assert_eq!(au.frame_type, EncodedFrameType::Key);
    }

    #[test]
    fn parser_splits_at_next_access_unit_delimiter() {
        let mut parser =
            AnnexBAccessUnitParser::new(EncodedVideoCodec::H264, 100, 33_333, 640, 480).unwrap();
        let stream =
            [0, 0, 1, 0x09, 0x10, 0, 0, 1, 0x65, 1, 2, 0, 0, 1, 0x09, 0x10, 0, 0, 1, 0x41, 3];

        let au = parser.push(&stream).unwrap().unwrap();
        assert_eq!(au.timestamp_us, 100);
        assert_eq!(au.payload.as_ref(), &[0, 0, 1, 0x09, 0x10, 0, 0, 1, 0x65, 1, 2]);

        let au = parser.flush().unwrap().unwrap();
        assert_eq!(au.timestamp_us, 33_433);
        assert_eq!(au.payload.as_ref(), &[0, 0, 1, 0x09, 0x10, 0, 0, 1, 0x41, 3]);
    }

    #[test]
    fn avc_parser_splits_at_next_access_unit_delimiter() {
        let mut parser = AvcAccessUnitParser::new(4, 100, 33_333, 640, 480).unwrap();
        let stream = [
            0, 0, 0, 2, 0x09, 0x10, 0, 0, 0, 3, 0x65, 1, 2, 0, 0, 0, 2, 0x09, 0x10, 0, 0, 0, 2,
            0x41, 3,
        ];

        let au = parser.push(&stream).unwrap().unwrap();
        assert_eq!(au.timestamp_us, 100);
        assert_eq!(au.payload.as_ref(), &[0, 0, 0, 1, 0x09, 0x10, 0, 0, 0, 1, 0x65, 1, 2]);

        let au = parser.flush().unwrap().unwrap();
        assert_eq!(au.timestamp_us, 33_433);
        assert_eq!(au.payload.as_ref(), &[0, 0, 0, 1, 0x09, 0x10, 0, 0, 0, 1, 0x41, 3]);
    }
}
