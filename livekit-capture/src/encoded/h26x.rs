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

//! H.264/H.265 parsing helpers: NAL-unit splitting, access-unit assembly,
//! and keyframe detection.

use crate::{
    encoded::{EncodedFrameType, EncodedVideoCodec, OwnedEncodedAccessUnit},
    primitive::VideoResolution,
};
use bytes::Bytes;
use std::ops::Range;
use thiserror::Error;

/// Error returned by the H26x parsing helpers.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum H26xParseError {
    /// Encoded payload is empty.
    #[error("encoded payload is empty")]
    EmptyPayload,
    /// H.265 NAL unit is too short to contain its header.
    #[error("H.265 NAL unit is too short")]
    H265NalTooShort,
    /// Codec is not supported by the H26x parsing helpers.
    #[error("H26x parsing does not support {0:?}")]
    UnsupportedCodec(EncodedVideoCodec),
    /// Encoded payload or transport data is malformed.
    #[error("invalid encoded data: {0}")]
    InvalidEncodedData(&'static str),
}

/// Start code prepended to each NAL unit when assembling Annex-B payloads.
const ANNEX_B_START_CODE: [u8; 4] = [0, 0, 0, 1];

/// Upper bound on bytes buffered while waiting for an access-unit boundary.
pub(crate) const MAX_PENDING_ACCESS_UNIT_BYTES: usize = 32 * 1024 * 1024;

/// Byte-stream access-unit parser shared by the encoded ingest sources.
///
/// `push` appends bytes and returns at most one completed access unit; call
/// `drain` repeatedly to pull further access units already buffered, and
/// `flush` once at end of stream to emit the final pending access unit.
#[cfg(test)]
pub(crate) trait AccessUnitParser {
    /// Appends bytes and returns the next complete access unit, if any.
    fn push(&mut self, bytes: &[u8]) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError>;

    /// Returns the next complete access unit from already-buffered bytes.
    fn drain(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        self.push(&[])
    }

    /// Flushes remaining buffered bytes as the final access unit.
    fn flush(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError>;
}

/// Incremental parser that splits an H.264 or H.265 Annex-B byte stream
/// into access units.
///
/// Call [`AnnexBAccessUnitParser::push`] as bytes arrive, and
/// [`AnnexBAccessUnitParser::flush`] at the end of the stream.
#[derive(Debug, Clone)]
pub struct AnnexBAccessUnitParser {
    codec: EncodedVideoCodec,
    pending: Vec<u8>,
    // NAL ranges found in `pending`; the last range's end is provisional
    // until the next start code (or flush) confirms it.
    nal_ranges: Vec<Range<usize>>,
    // Offset up to which `pending` has been scanned for start codes.
    scan_cursor: usize,
    next_timestamp_us: i64,
    frame_interval_us: i64,
    resolution: VideoResolution,
}

/// H.264/AVC length-prefixed parser state.
#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct AvcAccessUnitParser {
    pending: Vec<u8>,
    /// Complete NAL ranges found in `pending`.
    nal_ranges: Vec<Range<usize>>,
    /// Offset of the first unparsed length prefix or incomplete NAL in `pending`.
    scan_cursor: usize,
    nal_length_size: u8,
    next_timestamp_us: i64,
    frame_interval_us: i64,
    resolution: VideoResolution,
}

impl AnnexBAccessUnitParser {
    /// Creates a parser for an H.264 or H.265 Annex-B byte stream.
    ///
    /// Access units get timestamps that start at `start_timestamp_us` and
    /// step by `frame_interval_us`.
    pub fn new(
        codec: EncodedVideoCodec,
        start_timestamp_us: i64,
        frame_interval_us: i64,
        resolution: VideoResolution,
    ) -> Result<Self, H26xParseError> {
        match codec {
            EncodedVideoCodec::H264 | EncodedVideoCodec::H265 => {}
            EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
                return Err(H26xParseError::UnsupportedCodec(codec));
            }
        }

        Ok(Self {
            codec,
            pending: Vec::new(),
            nal_ranges: Vec::new(),
            scan_cursor: 0,
            next_timestamp_us: start_timestamp_us,
            frame_interval_us,
            resolution,
        })
    }

    /// Pushes encoded bytes and returns the next complete access unit, if
    /// one is found.
    ///
    /// One call returns at most one access unit. Push an empty slice to
    /// pull further access units that are already buffered.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        self.pending.extend_from_slice(bytes);
        self.drain_next(false)
    }

    /// Flushes the remaining buffered bytes as the final access unit.
    pub fn flush(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        self.drain_next(true)
    }

    fn drain_next(
        &mut self,
        at_eof: bool,
    ) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        self.scan_pending();

        if let Some(split_at) =
            access_unit_split_index(self.codec, &self.pending, &self.nal_ranges)?
        {
            return self.take_access_unit(split_at);
        }
        if at_eof && self.nal_ranges.iter().any(|range| range.start < range.end) {
            return self.take_access_unit(self.pending.len());
        }
        if !at_eof && self.pending.len() > MAX_PENDING_ACCESS_UNIT_BYTES {
            return Err(H26xParseError::InvalidEncodedData(
                "access unit exceeds maximum buffered size",
            ));
        }
        Ok(None)
    }

    /// Scans bytes appended since the previous call, extending the cached NAL ranges.
    fn scan_pending(&mut self) {
        // Resume behind the previous scan end so a start code straddling the
        // boundary is found, but never before the last NAL start so an
        // already-found start code is not rediscovered.
        let mut cursor = self.scan_cursor.saturating_sub(3);
        if let Some(last) = self.nal_ranges.last() {
            cursor = cursor.max(last.start);
        }
        while let Some((offset, prefix_len)) = find_start_code(&self.pending[cursor..]) {
            let prefix_start = cursor + offset;
            let nal_start = prefix_start + prefix_len;
            if let Some(last) = self.nal_ranges.last_mut() {
                last.end = prefix_start;
                if last.start >= prefix_start {
                    self.nal_ranges.pop();
                }
            }
            self.nal_ranges.push(nal_start..nal_start);
            cursor = nal_start;
        }
        if let Some(last) = self.nal_ranges.last_mut() {
            last.end = self.pending.len();
        }
        self.scan_cursor = self.pending.len();
    }

    fn take_access_unit(
        &mut self,
        byte_len: usize,
    ) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        if byte_len == 0 {
            return Ok(None);
        }

        let access_unit = self.pending[..byte_len].to_vec();
        self.pending.drain(..byte_len);
        self.nal_ranges.retain_mut(|range| {
            if range.end <= byte_len {
                return false;
            }
            range.start -= byte_len;
            range.end -= byte_len;
            true
        });
        self.scan_cursor -= byte_len;
        let timestamp_us = self.next_timestamp_us;
        self.next_timestamp_us = self.next_timestamp_us.saturating_add(self.frame_interval_us);
        access_unit_from_annex_b(
            self.codec,
            Bytes::from(access_unit),
            timestamp_us,
            self.resolution,
        )
        .map(Some)
    }
}

#[cfg(test)]
impl AccessUnitParser for AnnexBAccessUnitParser {
    fn push(&mut self, bytes: &[u8]) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        AnnexBAccessUnitParser::push(self, bytes)
    }

    fn flush(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        AnnexBAccessUnitParser::flush(self)
    }
}

#[cfg(test)]
impl AvcAccessUnitParser {
    /// Creates a parser for H.264/AVC length-prefixed byte streams.
    pub(crate) fn new(
        nal_length_size: u8,
        start_timestamp_us: i64,
        frame_interval_us: i64,
        resolution: VideoResolution,
    ) -> Result<Self, H26xParseError> {
        validate_avc_nal_length_size(nal_length_size)?;

        Ok(Self {
            pending: Vec::new(),
            nal_ranges: Vec::new(),
            scan_cursor: 0,
            nal_length_size,
            next_timestamp_us: start_timestamp_us,
            frame_interval_us,
            resolution,
        })
    }

    /// Pushes encoded bytes and returns the next complete access unit if one is found.
    pub(crate) fn push(
        &mut self,
        bytes: &[u8],
    ) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        self.pending.extend_from_slice(bytes);
        self.drain_next(false)
    }

    /// Flushes the pending bytes as the final access unit.
    pub(crate) fn flush(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        self.drain_next(true)
    }

    fn drain_next(
        &mut self,
        at_eof: bool,
    ) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        self.scan_pending(at_eof)?;

        if let Some(split_at) = avc_access_unit_split_index(
            &self.pending,
            &self.nal_ranges,
            self.nal_length_size as usize,
        )? {
            return self.take_access_unit(split_at);
        }
        if at_eof && !self.nal_ranges.is_empty() {
            return self.take_access_unit(self.pending.len());
        }
        if !at_eof && self.pending.len() > MAX_PENDING_ACCESS_UNIT_BYTES {
            return Err(H26xParseError::InvalidEncodedData(
                "access unit exceeds maximum buffered size",
            ));
        }
        Ok(None)
    }

    /// Parses length-prefixed NAL units appended since the previous call.
    fn scan_pending(&mut self, at_eof: bool) -> Result<(), H26xParseError> {
        let nal_length_size = self.nal_length_size as usize;
        while self.scan_cursor < self.pending.len() {
            if self.pending.len() - self.scan_cursor < nal_length_size {
                if at_eof {
                    return Err(H26xParseError::InvalidEncodedData("truncated AVC NAL length"));
                }
                break;
            }

            let nal_start = self.scan_cursor + nal_length_size;
            let nal_len = read_avc_nal_length(&self.pending[self.scan_cursor..nal_start]);
            if nal_len == 0 {
                return Err(H26xParseError::InvalidEncodedData("empty AVC NAL unit"));
            }

            let Some(nal_end) = nal_start.checked_add(nal_len) else {
                return Err(H26xParseError::InvalidEncodedData("AVC NAL unit length overflow"));
            };
            if nal_end > self.pending.len() {
                if at_eof {
                    return Err(H26xParseError::InvalidEncodedData("truncated AVC NAL unit"));
                }
                break;
            }

            self.nal_ranges.push(nal_start..nal_end);
            self.scan_cursor = nal_end;
        }
        Ok(())
    }

    fn take_access_unit(
        &mut self,
        byte_len: usize,
    ) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        if byte_len == 0 {
            return Ok(None);
        }

        let access_unit = self.pending[..byte_len].to_vec();
        self.pending.drain(..byte_len);
        self.nal_ranges.retain_mut(|range| {
            if range.end <= byte_len {
                return false;
            }
            range.start -= byte_len;
            range.end -= byte_len;
            true
        });
        self.scan_cursor -= byte_len;
        let timestamp_us = self.next_timestamp_us;
        self.next_timestamp_us = self.next_timestamp_us.saturating_add(self.frame_interval_us);
        access_unit_from_h264_avc(&access_unit, self.nal_length_size, timestamp_us, self.resolution)
            .map(Some)
    }
}

#[cfg(test)]
impl AccessUnitParser for AvcAccessUnitParser {
    fn push(&mut self, bytes: &[u8]) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        AvcAccessUnitParser::push(self, bytes)
    }

    fn flush(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        AvcAccessUnitParser::flush(self)
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
pub fn annex_b_nalus(bytes: &[u8]) -> Vec<&[u8]> {
    annex_b_nal_ranges(bytes)
        .into_iter()
        .map(|range| &bytes[range])
        .filter(|nal| !nal.is_empty())
        .collect()
}

/// Creates an Annex-B access unit from H.264/AVC length-prefixed NAL units.
pub fn access_unit_from_h264_avc(
    payload: &[u8],
    nal_length_size: u8,
    timestamp_us: i64,
    resolution: VideoResolution,
) -> Result<OwnedEncodedAccessUnit, H26xParseError> {
    let nals = avc_nalus(payload, nal_length_size)?;
    access_unit_from_nalus(EncodedVideoCodec::H264, &nals, timestamp_us, resolution)
}

/// Creates an access unit from an Annex-B buffer.
pub fn access_unit_from_annex_b(
    codec: EncodedVideoCodec,
    payload: Bytes,
    timestamp_us: i64,
    resolution: VideoResolution,
) -> Result<OwnedEncodedAccessUnit, H26xParseError> {
    if payload.is_empty() {
        return Err(H26xParseError::EmptyPayload);
    }

    let frame_type = if is_keyframe_annex_b(codec, &payload)? {
        EncodedFrameType::Key
    } else {
        EncodedFrameType::Delta
    };
    Ok(OwnedEncodedAccessUnit::new(codec, payload, timestamp_us, frame_type, resolution))
}

/// Creates an Annex-B access unit from raw NAL units.
pub fn access_unit_from_nalus(
    codec: EncodedVideoCodec,
    nal_units: &[&[u8]],
    timestamp_us: i64,
    resolution: VideoResolution,
) -> Result<OwnedEncodedAccessUnit, H26xParseError> {
    let payload = Bytes::from(annex_b_payload(nal_units)?);
    access_unit_from_annex_b(codec, payload, timestamp_us, resolution)
}

/// Returns `true` when an Annex-B access unit is a key frame: an IDR
/// picture for H.264, or parameter sets (VPS/SPS/PPS) plus an IDR picture
/// for H.265.
pub fn is_keyframe_annex_b(codec: EncodedVideoCodec, bytes: &[u8]) -> Result<bool, H26xParseError> {
    let nals = annex_b_nalus(bytes);
    is_keyframe_nalus(codec, &nals)
}

/// Returns true when the NAL units form a WebRTC-usable key frame.
fn is_keyframe_nalus(
    codec: EncodedVideoCodec,
    nal_units: &[&[u8]],
) -> Result<bool, H26xParseError> {
    match codec {
        EncodedVideoCodec::H264 => {
            nal_units.iter().try_fold(false, |is_key, nal| Ok(is_key || h264_nal_type(nal)? == 5))
        }
        EncodedVideoCodec::H265 => {
            let mut has_vps = false;
            let mut has_sps = false;
            let mut has_pps = false;
            let mut has_idr = false;

            for nal in nal_units {
                match h265_nal_type(nal)? {
                    32 => has_vps = true,
                    33 => has_sps = true,
                    34 => has_pps = true,
                    19 | 20 => has_idr = true,
                    _ => {}
                }
            }

            Ok(has_vps && has_sps && has_pps && has_idr)
        }
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
            Err(H26xParseError::UnsupportedCodec(codec))
        }
    }
}

/// Extracts the type of an H.264 NAL unit from its header.
pub(crate) fn h264_nal_type(nal: &[u8]) -> Result<u8, H26xParseError> {
    let header = nal.first().ok_or(H26xParseError::EmptyPayload)?;
    Ok(header & 0x1f)
}

/// Extracts the type of an H.265 NAL unit from its header.
pub(crate) fn h265_nal_type(nal: &[u8]) -> Result<u8, H26xParseError> {
    if nal.is_empty() {
        return Err(H26xParseError::EmptyPayload);
    }
    if nal.len() < 2 {
        return Err(H26xParseError::H265NalTooShort);
    }
    Ok((nal[0] >> 1) & 0x3f)
}

fn annex_b_payload(nal_units: &[&[u8]]) -> Result<Vec<u8>, H26xParseError> {
    if nal_units.is_empty() {
        return Err(H26xParseError::EmptyPayload);
    }
    let len = nal_units.iter().try_fold(0usize, |len, nal| {
        if nal.is_empty() {
            Err(H26xParseError::EmptyPayload)
        } else {
            Ok(len + ANNEX_B_START_CODE.len() + nal.len())
        }
    })?;

    let mut payload = Vec::with_capacity(len);
    for nal in nal_units {
        payload.extend_from_slice(&ANNEX_B_START_CODE);
        payload.extend_from_slice(nal);
    }
    Ok(payload)
}

fn access_unit_split_index(
    codec: EncodedVideoCodec,
    bytes: &[u8],
    ranges: &[Range<usize>],
) -> Result<Option<usize>, H26xParseError> {
    match access_unit_boundary_nal(codec, bytes, ranges)? {
        Some(index) => split_start_code_index(bytes, ranges[index].start).map(Some),
        None => Ok(None),
    }
}

#[cfg(test)]
fn avc_access_unit_split_index(
    bytes: &[u8],
    ranges: &[Range<usize>],
    nal_length_size: usize,
) -> Result<Option<usize>, H26xParseError> {
    match access_unit_boundary_nal(EncodedVideoCodec::H264, bytes, ranges)? {
        Some(index) => ranges[index]
            .start
            .checked_sub(nal_length_size)
            .ok_or(H26xParseError::InvalidEncodedData("missing AVC NAL length"))
            .map(Some),
        None => Ok(None),
    }
}

/// Returns the index of the first NAL that starts a new access unit, once at
/// least one VCL NAL has been seen in the current one.
fn access_unit_boundary_nal(
    codec: EncodedVideoCodec,
    bytes: &[u8],
    ranges: &[Range<usize>],
) -> Result<Option<usize>, H26xParseError> {
    let mut seen_vcl = false;
    for (index, range) in ranges.iter().enumerate() {
        let nal = &bytes[range.clone()];
        // The final NAL may still be streaming in; wait for its header.
        if index + 1 == ranges.len() && nal.len() < min_nal_header_len(codec) {
            return Ok(None);
        }
        if seen_vcl && starts_new_access_unit(codec, nal)? {
            return Ok(Some(index));
        }
        seen_vcl |= is_vcl_nal(codec, nal)?;
    }
    Ok(None)
}

fn min_nal_header_len(codec: EncodedVideoCodec) -> usize {
    match codec {
        EncodedVideoCodec::H265 => 2,
        _ => 1,
    }
}

fn starts_new_access_unit(codec: EncodedVideoCodec, nal: &[u8]) -> Result<bool, H26xParseError> {
    Ok(match codec {
        EncodedVideoCodec::H264 => match h264_nal_type(nal)? {
            // Prefix SEI(6), SPS(7), PPS(8), and AUD(9) open a new access unit.
            6..=9 => true,
            // A VCL NAL opens a new picture when first_mb_in_slice == 0:
            // ue(v) == 0 is a lone 1 bit, so the first RBSP bit after the
            // header is set. The header byte is nonzero, so the next byte
            // cannot be an emulation-prevention byte.
            1..=5 => nal.len() >= 2 && nal[1] & 0x80 != 0,
            _ => false,
        },
        EncodedVideoCodec::H265 => match h265_nal_type(nal)? {
            // VPS(32), SPS(33), PPS(34), AUD(35), and prefix SEI(39).
            32..=35 | 39 => true,
            // A VCL NAL opens a new picture when
            // first_slice_segment_in_pic_flag (the bit after the 2-byte
            // header) is set. nuh_temporal_id_plus1 makes the second header
            // byte nonzero, so the next byte cannot be an
            // emulation-prevention byte.
            0..=31 => nal.len() >= 3 && nal[2] & 0x80 != 0,
            _ => false,
        },
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
            return Err(H26xParseError::UnsupportedCodec(codec));
        }
    })
}

fn split_start_code_index(bytes: &[u8], nal_start: usize) -> Result<usize, H26xParseError> {
    if nal_start >= 4 && bytes[nal_start - 4..nal_start] == [0, 0, 0, 1] {
        return Ok(nal_start - 4);
    }
    if nal_start >= 3 && bytes[nal_start - 3..nal_start] == [0, 0, 1] {
        return Ok(nal_start - 3);
    }
    Err(H26xParseError::InvalidEncodedData("missing Annex-B start code"))
}

fn is_vcl_nal(codec: EncodedVideoCodec, nal: &[u8]) -> Result<bool, H26xParseError> {
    Ok(match codec {
        EncodedVideoCodec::H264 => (1..=5).contains(&h264_nal_type(nal)?),
        EncodedVideoCodec::H265 => h265_nal_type(nal)? <= 31,
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
            return Err(H26xParseError::UnsupportedCodec(codec));
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

/// Splits a sequence of length-prefixed NAL units, as used by AVC-format
/// access units and, with a 2-byte length, RTP aggregation packets.
pub(crate) fn avc_nalus(payload: &[u8], nal_length_size: u8) -> Result<Vec<&[u8]>, H26xParseError> {
    let ranges = avc_nal_ranges(payload, nal_length_size, true)?;
    if ranges.is_empty() {
        return Err(H26xParseError::EmptyPayload);
    }
    Ok(ranges.into_iter().map(|range| &payload[range]).collect())
}

fn avc_nal_ranges(
    bytes: &[u8],
    nal_length_size: u8,
    at_eof: bool,
) -> Result<Vec<Range<usize>>, H26xParseError> {
    validate_avc_nal_length_size(nal_length_size)?;

    let nal_length_size = nal_length_size as usize;
    let mut ranges = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes.len() - cursor < nal_length_size {
            if at_eof {
                return Err(H26xParseError::InvalidEncodedData("truncated AVC NAL length"));
            }
            break;
        }

        let nal_len = read_avc_nal_length(&bytes[cursor..cursor + nal_length_size]);
        cursor += nal_length_size;
        if nal_len == 0 {
            return Err(H26xParseError::InvalidEncodedData("empty AVC NAL unit"));
        }

        let Some(nal_end) = cursor.checked_add(nal_len) else {
            return Err(H26xParseError::InvalidEncodedData("AVC NAL unit length overflow"));
        };
        if nal_end > bytes.len() {
            if at_eof {
                return Err(H26xParseError::InvalidEncodedData("truncated AVC NAL unit"));
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

fn validate_avc_nal_length_size(nal_length_size: u8) -> Result<(), H26xParseError> {
    if (1..=4).contains(&nal_length_size) {
        return Ok(());
    }
    Err(H26xParseError::InvalidEncodedData("invalid AVC NAL length size"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h264_keyframe_requires_idr_nal() {
        let sps = [0x67, 1, 2, 3];
        let idr = [0x65, 4, 5, 6];
        let non_idr = [0x61, 1, 2];

        assert!(is_keyframe_nalus(EncodedVideoCodec::H264, &[&sps, &idr]).unwrap());
        assert!(!is_keyframe_nalus(EncodedVideoCodec::H264, &[&sps, &non_idr]).unwrap());
    }

    #[test]
    fn h265_keyframe_requires_parameter_sets_and_idr() {
        let vps = [0x40, 1, 2];
        let sps = [0x42, 1, 2];
        let pps = [0x44, 1, 2];
        let idr_w_radl = [19 << 1, 1, 3];
        let cra = [21 << 1, 1, 3];

        assert!(!is_keyframe_nalus(EncodedVideoCodec::H265, &[&vps, &idr_w_radl]).unwrap());
        assert!(
            is_keyframe_nalus(EncodedVideoCodec::H265, &[&vps, &sps, &pps, &idr_w_radl]).unwrap()
        );
        assert!(!is_keyframe_nalus(EncodedVideoCodec::H265, &[&vps, &sps, &pps, &cra]).unwrap());
    }

    #[test]
    fn h265_rejects_too_short_nal_header() {
        let err = is_keyframe_nalus(EncodedVideoCodec::H265, &[&[0x26]]).unwrap_err();
        assert_eq!(err, H26xParseError::H265NalTooShort);
    }

    #[test]
    fn annex_b_payload_prefixes_each_nal_unit() {
        let payload = annex_b_payload(&[&[0x67, 1, 2, 3], &[0x65, 4, 5, 6]]).unwrap();
        assert_eq!(payload, vec![0, 0, 0, 1, 0x67, 1, 2, 3, 0, 0, 0, 1, 0x65, 4, 5, 6]);
    }

    #[test]
    fn annex_b_payload_rejects_empty_input() {
        assert_eq!(annex_b_payload(&[]).unwrap_err(), H26xParseError::EmptyPayload);
        assert_eq!(annex_b_payload(&[&[]]).unwrap_err(), H26xParseError::EmptyPayload);
    }

    #[test]
    fn splits_annex_b_nals_with_three_and_four_byte_prefixes() {
        let bytes = [0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x65, 2, 3];
        let nals = annex_b_nalus(&bytes);
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
        let au = access_unit_from_h264_avc(&bytes, 4, 10, VideoResolution::new(640, 480)).unwrap();

        assert_eq!(au.codec, EncodedVideoCodec::H264);
        assert_eq!(au.frame_type, EncodedFrameType::Key);
        assert_eq!(au.payload.as_ref(), &[0, 0, 0, 1, 0x67, 1, 2, 3, 0, 0, 0, 1, 0x65, 4, 5]);
    }

    #[test]
    fn access_unit_from_avc_supports_two_byte_lengths() {
        let bytes = [0, 2, 0x61, 1];
        let au = access_unit_from_h264_avc(&bytes, 2, 10, VideoResolution::new(640, 480)).unwrap();

        assert_eq!(au.frame_type, EncodedFrameType::Delta);
        assert_eq!(au.payload.as_ref(), &[0, 0, 0, 1, 0x61, 1]);
    }

    #[test]
    fn access_unit_from_avc_rejects_truncated_nal() {
        let err =
            access_unit_from_h264_avc(&[0, 0, 0, 3, 0x65], 4, 10, VideoResolution::new(640, 480))
                .unwrap_err();

        assert_eq!(err, H26xParseError::InvalidEncodedData("truncated AVC NAL unit"));
    }

    #[test]
    fn parser_flushes_final_access_unit() {
        let mut parser = AnnexBAccessUnitParser::new(
            EncodedVideoCodec::H264,
            100,
            33_333,
            VideoResolution::new(640, 480),
        )
        .unwrap();
        assert!(parser.push(&[0, 0, 1, 0x65, 1, 2]).unwrap().is_none());
        let au = parser.flush().unwrap().unwrap();
        assert_eq!(au.timestamp_us, 100);
        assert_eq!(au.frame_type, EncodedFrameType::Key);
    }

    #[test]
    fn parser_splits_at_next_access_unit_delimiter() {
        let mut parser = AnnexBAccessUnitParser::new(
            EncodedVideoCodec::H264,
            100,
            33_333,
            VideoResolution::new(640, 480),
        )
        .unwrap();
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
        let mut parser =
            AvcAccessUnitParser::new(4, 100, 33_333, VideoResolution::new(640, 480)).unwrap();
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

    #[test]
    fn splits_aud_less_h264_stream_per_frame() {
        let mut parser = AnnexBAccessUnitParser::new(
            EncodedVideoCodec::H264,
            0,
            33_333,
            VideoResolution::new(640, 480),
        )
        .unwrap();
        let stream = [
            0, 0, 0, 1, 0x67, 0x42, 0x00, 0x1e, // SPS
            0, 0, 0, 1, 0x68, 0xce, // PPS
            0, 0, 1, 0x65, 0x88, 0x84, 0x21, // IDR slice, first_mb_in_slice == 0
            0, 0, 1, 0x41, 0x9a, 0x22, // P slice, first_mb_in_slice == 0
            0, 0, 1, 0x41, 0x9a, 0x33, // P slice, first_mb_in_slice == 0
        ];

        let au = parser.push(&stream).unwrap().unwrap();
        assert_eq!(au.timestamp_us, 0);
        assert_eq!(au.frame_type, EncodedFrameType::Key);
        assert_eq!(au.payload.as_ref(), &stream[..21]);

        let au = parser.drain().unwrap().unwrap();
        assert_eq!(au.timestamp_us, 33_333);
        assert_eq!(au.frame_type, EncodedFrameType::Delta);
        assert_eq!(au.payload.as_ref(), &stream[21..27]);

        let au = parser.flush().unwrap().unwrap();
        assert_eq!(au.timestamp_us, 66_666);
        assert_eq!(au.payload.as_ref(), &stream[27..]);
    }

    #[test]
    fn keeps_multi_slice_h264_access_unit_together() {
        let mut parser = AnnexBAccessUnitParser::new(
            EncodedVideoCodec::H264,
            0,
            33_333,
            VideoResolution::new(640, 480),
        )
        .unwrap();
        let stream = [
            0, 0, 1, 0x65, 0x88, 0x11, // IDR slice, first_mb_in_slice == 0
            0, 0, 1, 0x65, 0x21, 0x22, // IDR slice, first_mb_in_slice != 0
            0, 0, 1, 0x41, 0x9a, 0x33, // next picture
        ];

        let au = parser.push(&stream).unwrap().unwrap();
        assert_eq!(au.timestamp_us, 0);
        assert_eq!(au.frame_type, EncodedFrameType::Key);
        assert_eq!(au.payload.as_ref(), &stream[..12]);

        let au = parser.flush().unwrap().unwrap();
        assert_eq!(au.timestamp_us, 33_333);
        assert_eq!(au.payload.as_ref(), &stream[12..]);
    }

    #[test]
    fn splits_aud_less_h265_stream_per_frame() {
        let mut parser = AnnexBAccessUnitParser::new(
            EncodedVideoCodec::H265,
            0,
            33_333,
            VideoResolution::new(640, 480),
        )
        .unwrap();
        let stream = [
            0, 0, 0, 1, 0x40, 0x01, 0x0c, // VPS
            0, 0, 0, 1, 0x42, 0x01, 0x02, // SPS
            0, 0, 0, 1, 0x44, 0x01, 0x03, // PPS
            0, 0, 1, 0x26, 0x01, 0xaf,
            0x04, // IDR_W_RADL, first_slice_segment_in_pic_flag == 1
            0, 0, 1, 0x02, 0x01, 0xd0, 0x05, // TRAIL_R, first_slice_segment_in_pic_flag == 1
        ];

        let au = parser.push(&stream).unwrap().unwrap();
        assert_eq!(au.timestamp_us, 0);
        assert_eq!(au.frame_type, EncodedFrameType::Key);
        assert_eq!(au.payload.as_ref(), &stream[..28]);

        let au = parser.flush().unwrap().unwrap();
        assert_eq!(au.timestamp_us, 33_333);
        assert_eq!(au.frame_type, EncodedFrameType::Delta);
        assert_eq!(au.payload.as_ref(), &stream[28..]);
    }

    #[test]
    fn keeps_multi_slice_h265_access_unit_together() {
        let mut parser = AnnexBAccessUnitParser::new(
            EncodedVideoCodec::H265,
            0,
            33_333,
            VideoResolution::new(640, 480),
        )
        .unwrap();
        let stream = [
            0, 0, 1, 0x26, 0x01, 0xaf,
            0x11, // IDR slice, first_slice_segment_in_pic_flag == 1
            0, 0, 1, 0x26, 0x01, 0x40,
            0x22, // IDR slice, first_slice_segment_in_pic_flag == 0
            0, 0, 1, 0x02, 0x01, 0xd0, 0x33, // next picture
        ];

        let au = parser.push(&stream).unwrap().unwrap();
        assert_eq!(au.timestamp_us, 0);
        assert_eq!(au.frame_type, EncodedFrameType::Delta);
        assert_eq!(au.payload.as_ref(), &stream[..14]);

        let au = parser.flush().unwrap().unwrap();
        assert_eq!(au.timestamp_us, 33_333);
        assert_eq!(au.payload.as_ref(), &stream[14..]);
    }

    #[test]
    fn groups_parameter_sets_with_following_frame() {
        let mut parser = AnnexBAccessUnitParser::new(
            EncodedVideoCodec::H264,
            0,
            33_333,
            VideoResolution::new(640, 480),
        )
        .unwrap();
        let stream = [
            0, 0, 1, 0x67, 0x42, 0x1e, // SPS
            0, 0, 1, 0x68, 0xce, // PPS
            0, 0, 1, 0x65, 0x88, 0x11, // IDR
            0, 0, 1, 0x67, 0x42, 0x1e, // SPS
            0, 0, 1, 0x68, 0xce, // PPS
            0, 0, 1, 0x65, 0x88, 0x22, // IDR
        ];

        let au = parser.push(&stream).unwrap().unwrap();
        assert_eq!(au.timestamp_us, 0);
        assert_eq!(au.frame_type, EncodedFrameType::Key);
        assert_eq!(au.payload.as_ref(), &stream[..17]);

        let au = parser.flush().unwrap().unwrap();
        assert_eq!(au.timestamp_us, 33_333);
        assert_eq!(au.frame_type, EncodedFrameType::Key);
        assert_eq!(au.payload.as_ref(), &stream[17..]);
    }

    fn collect_units(
        parser: &mut impl AccessUnitParser,
        stream: &[u8],
        chunk_size: usize,
    ) -> Vec<(Vec<u8>, i64, EncodedFrameType)> {
        let mut units = Vec::new();
        for chunk in stream.chunks(chunk_size) {
            let mut unit = parser.push(chunk).unwrap();
            while let Some(au) = unit {
                units.push((au.payload.to_vec(), au.timestamp_us, au.frame_type));
                unit = parser.drain().unwrap();
            }
        }
        let mut unit = parser.flush().unwrap();
        while let Some(au) = unit {
            units.push((au.payload.to_vec(), au.timestamp_us, au.frame_type));
            unit = parser.flush().unwrap();
        }
        units
    }

    fn assert_chunked_matches_one_shot<P: AccessUnitParser>(
        make_parser: impl Fn() -> P,
        stream: &[u8],
        expected_units: usize,
    ) {
        let baseline = collect_units(&mut make_parser(), stream, stream.len());
        assert_eq!(baseline.len(), expected_units);
        for chunk_size in [1, 7] {
            assert_eq!(collect_units(&mut make_parser(), stream, chunk_size), baseline);
        }
    }

    #[test]
    fn chunked_pushes_match_one_shot_parsing() {
        let h264_annex_b = [
            0, 0, 0, 1, 0x67, 0x64, 0x00, 0x1e, // SPS
            0, 0, 0, 1, 0x68, 0xce, 0x3c, 0x80, // PPS
            0, 0, 1, 0x65, 0x88, 0x84, 0x00, 0x01, // IDR, first_mb_in_slice == 0
            0, 0, 1, 0x41, 0x9a, 0x02, // P, first_mb_in_slice == 0
            0, 0, 1, 0x09, 0x10, // AUD
            0, 0, 1, 0x41, 0x9a, 0x03, // P
            0, 0, 0, 1, 0x41, 0x9a, 0x04, 0x00, // P, first_mb_in_slice == 0
        ];
        assert_chunked_matches_one_shot(
            || {
                AnnexBAccessUnitParser::new(
                    EncodedVideoCodec::H264,
                    0,
                    33_333,
                    VideoResolution::new(640, 480),
                )
                .unwrap()
            },
            &h264_annex_b,
            4,
        );

        let h265_annex_b = [
            0, 0, 0, 1, 0x40, 0x01, 0x0c, // VPS
            0, 0, 0, 1, 0x42, 0x01, 0x02, // SPS
            0, 0, 0, 1, 0x44, 0x01, 0x03, // PPS
            0, 0, 1, 0x26, 0x01, 0xaf, 0x08, // IDR_W_RADL
            0, 0, 1, 0x02, 0x01, 0xd0, 0x09, // TRAIL_R
            0, 0, 1, 0x46, 0x01, 0x50, // AUD
            0, 0, 1, 0x02, 0x01, 0xd0, 0x0a, // TRAIL_R
        ];
        assert_chunked_matches_one_shot(
            || {
                AnnexBAccessUnitParser::new(
                    EncodedVideoCodec::H265,
                    0,
                    33_333,
                    VideoResolution::new(640, 480),
                )
                .unwrap()
            },
            &h265_annex_b,
            3,
        );

        let h264_avc = [
            0, 0, 0, 4, 0x67, 0x64, 0x00, 0x1e, // SPS
            0, 0, 0, 2, 0x68, 0xce, // PPS
            0, 0, 0, 4, 0x65, 0x88, 0x84, 0x00, // IDR, first_mb_in_slice == 0
            0, 0, 0, 3, 0x41, 0x9a, 0x02, // P, first_mb_in_slice == 0
            0, 0, 0, 2, 0x09, 0x10, // AUD
            0, 0, 0, 3, 0x41, 0x9a, 0x03, // P
        ];
        assert_chunked_matches_one_shot(
            || AvcAccessUnitParser::new(4, 0, 33_333, VideoResolution::new(640, 480)).unwrap(),
            &h264_avc,
            3,
        );
    }

    #[test]
    fn rejects_pending_access_unit_over_size_cap() {
        let mut parser = AnnexBAccessUnitParser::new(
            EncodedVideoCodec::H264,
            0,
            33_333,
            VideoResolution::new(640, 480),
        )
        .unwrap();
        assert!(parser.push(&[0, 0, 1, 0x65, 0x88]).unwrap().is_none());

        let err = parser.push(&vec![0xff; MAX_PENDING_ACCESS_UNIT_BYTES]).unwrap_err();
        assert_eq!(
            err,
            H26xParseError::InvalidEncodedData("access unit exceeds maximum buffered size")
        );
    }

    #[test]
    fn avc_rejects_pending_access_unit_over_size_cap() {
        let mut parser =
            AvcAccessUnitParser::new(4, 0, 33_333, VideoResolution::new(640, 480)).unwrap();
        let nal_len = (MAX_PENDING_ACCESS_UNIT_BYTES + 1) as u32;
        assert!(parser.push(&nal_len.to_be_bytes()).unwrap().is_none());

        let err = parser.push(&vec![0x41; MAX_PENDING_ACCESS_UNIT_BYTES]).unwrap_err();
        assert_eq!(
            err,
            H26xParseError::InvalidEncodedData("access unit exceeds maximum buffered size")
        );
    }
}
