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
use livekit::webrtc::video_frame::FrameMetadata;
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
const MAX_PENDING_ACCESS_UNIT_BYTES: usize = 32 * 1024 * 1024;

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

/// Incremental parser that splits an H.264 (AVC) or H.265 (HVCC) stream of
/// length-prefixed NAL units into access units.
///
/// This is the framing of `stream-format=avc3` (H.264) and `hev1` (H.265),
/// with parameter sets in-band. Each access unit is converted to Annex-B.
///
/// Call [`LengthPrefixedAccessUnitParser::push`] as bytes arrive, and
/// [`LengthPrefixedAccessUnitParser::flush`] at the end of the stream.
#[derive(Debug, Clone)]
pub struct LengthPrefixedAccessUnitParser {
    codec: EncodedVideoCodec,
    pending: Vec<u8>,
    /// Complete NAL ranges found in `pending`.
    nal_ranges: Vec<Range<usize>>,
    /// Offset of the first unparsed length prefix or incomplete NAL in `pending`.
    scan_cursor: usize,
    nal_length_size: u8,
    single_slice_flush: bool,
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

impl LengthPrefixedAccessUnitParser {
    /// Creates a parser for an H.264 or H.265 length-prefixed byte stream.
    ///
    /// `nal_length_size` is the size of each big-endian NAL length prefix,
    /// 1 to 4 bytes (`lengthSizeMinusOne + 1` from `avcC`/`hvcC`; 4 for most
    /// encoders). Access units get timestamps that start at
    /// `start_timestamp_us` and step by `frame_interval_us`.
    pub fn new(
        codec: EncodedVideoCodec,
        nal_length_size: u8,
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
        validate_nal_length_size(nal_length_size)?;

        Ok(Self {
            codec,
            pending: Vec::new(),
            nal_ranges: Vec::new(),
            scan_cursor: 0,
            nal_length_size,
            single_slice_flush: false,
            next_timestamp_us: start_timestamp_us,
            frame_interval_us,
            resolution,
        })
    }

    /// Completes each access unit as soon as its first slice arrives,
    /// instead of when the next access unit begins.
    ///
    /// The end of an access unit is otherwise only known from the NAL that
    /// starts the next one, which holds every picture for a full frame
    /// interval unless the encoder terminates access units with an AUD.
    /// Enable this only for streams with one slice per picture, which is
    /// what hardware encoders typically emit: with several slices per
    /// picture, the remaining slices would land in the next access unit.
    pub fn with_single_slice_flush(mut self, enabled: bool) -> Self {
        self.single_slice_flush = enabled;
        self
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
        self.scan_pending(at_eof)?;

        if let Some(split_at) = self.split_index()? {
            return self.take_access_unit(split_at);
        }
        if at_eof && !self.nal_ranges.is_empty() {
            // A tail without a slice (e.g. a trailing AUD) is not a picture.
            if !self.has_vcl_nal()? {
                self.pending.clear();
                self.nal_ranges.clear();
                self.scan_cursor = 0;
                return Ok(None);
            }
            return self.take_access_unit(self.pending.len());
        }
        if !at_eof && self.pending.len() > MAX_PENDING_ACCESS_UNIT_BYTES {
            return Err(H26xParseError::InvalidEncodedData(
                "access unit exceeds maximum buffered size",
            ));
        }
        Ok(None)
    }

    fn has_vcl_nal(&self) -> Result<bool, H26xParseError> {
        for range in &self.nal_ranges {
            if is_vcl_nal(self.codec, &self.pending[range.clone()])? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns the byte offset that ends the current access unit, if known.
    fn split_index(&self) -> Result<Option<usize>, H26xParseError> {
        let nal_length_size = self.nal_length_size as usize;
        if self.single_slice_flush {
            // Scanned ranges only cover complete NALs, so the first VCL NAL
            // already holds the whole picture.
            for range in &self.nal_ranges {
                if is_vcl_nal(self.codec, &self.pending[range.clone()])? {
                    return Ok(Some(range.end));
                }
            }
            return Ok(None);
        }
        match access_unit_boundary_nal(self.codec, &self.pending, &self.nal_ranges)? {
            Some(index) => self.nal_ranges[index]
                .start
                .checked_sub(nal_length_size)
                .ok_or(H26xParseError::InvalidEncodedData("missing NAL length prefix"))
                .map(Some),
            None => Ok(None),
        }
    }

    /// Parses length-prefixed NAL units appended since the previous call.
    fn scan_pending(&mut self, at_eof: bool) -> Result<(), H26xParseError> {
        let nal_length_size = self.nal_length_size as usize;
        while self.scan_cursor < self.pending.len() {
            if self.pending.len() - self.scan_cursor < nal_length_size {
                if at_eof {
                    return Err(H26xParseError::InvalidEncodedData("truncated NAL length prefix"));
                }
                break;
            }

            let nal_start = self.scan_cursor + nal_length_size;
            let nal_len = read_nal_length(&self.pending[self.scan_cursor..nal_start]);
            if nal_len == 0 {
                return Err(H26xParseError::InvalidEncodedData("empty length-prefixed NAL unit"));
            }

            let Some(nal_end) = nal_start.checked_add(nal_len) else {
                return Err(H26xParseError::InvalidEncodedData("NAL unit length overflow"));
            };
            if nal_end > self.pending.len() {
                if at_eof {
                    return Err(H26xParseError::InvalidEncodedData(
                        "truncated length-prefixed NAL unit",
                    ));
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

        // The NAL ranges are already known: build the Annex-B payload and
        // classify the frame from them instead of re-parsing the bytes.
        let nal_count = self.nal_ranges.iter().take_while(|range| range.end <= byte_len).count();
        let nals = &self.nal_ranges[..nal_count];
        let nal_units: Vec<&[u8]> = nals.iter().map(|range| &self.pending[range.clone()]).collect();
        let frame_type = if is_keyframe_nalus(self.codec, &nal_units)? {
            EncodedFrameType::Key
        } else {
            EncodedFrameType::Delta
        };
        let payload = if self.nal_length_size as usize == ANNEX_B_START_CODE.len() {
            // A 4-byte prefix is the size of a start code: convert in place
            // during the one copy out of the pending buffer.
            let mut payload = self.pending[..byte_len].to_vec();
            for range in nals {
                payload[range.start - ANNEX_B_START_CODE.len()..range.start]
                    .copy_from_slice(&ANNEX_B_START_CODE);
            }
            payload
        } else {
            annex_b_payload(&nal_units)?
        };

        self.pending.drain(..byte_len);
        self.nal_ranges.drain(..nal_count);
        for range in &mut self.nal_ranges {
            range.start -= byte_len;
            range.end -= byte_len;
        }
        self.scan_cursor -= byte_len;
        let timestamp_us = self.next_timestamp_us;
        self.next_timestamp_us = self.next_timestamp_us.saturating_add(self.frame_interval_us);
        Ok(Some(OwnedEncodedAccessUnit::new(
            self.codec,
            payload,
            timestamp_us,
            frame_type,
            self.resolution,
        )))
    }
}

#[cfg(test)]
impl AccessUnitParser for LengthPrefixedAccessUnitParser {
    fn push(&mut self, bytes: &[u8]) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        LengthPrefixedAccessUnitParser::push(self, bytes)
    }

    fn flush(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, H26xParseError> {
        LengthPrefixedAccessUnitParser::flush(self)
    }
}

/// Returns NAL-unit byte ranges for an Annex-B access unit or stream chunk.
pub(crate) fn annex_b_nal_ranges(bytes: &[u8]) -> Vec<Range<usize>> {
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
pub(crate) fn annex_b_nalus(bytes: &[u8]) -> Vec<&[u8]> {
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
    access_unit_from_length_prefixed(
        EncodedVideoCodec::H264,
        payload,
        nal_length_size,
        timestamp_us,
        resolution,
    )
}

/// Creates an Annex-B access unit from H.265/HVCC length-prefixed NAL units.
pub fn access_unit_from_h265_hvcc(
    payload: &[u8],
    nal_length_size: u8,
    timestamp_us: i64,
    resolution: VideoResolution,
) -> Result<OwnedEncodedAccessUnit, H26xParseError> {
    access_unit_from_length_prefixed(
        EncodedVideoCodec::H265,
        payload,
        nal_length_size,
        timestamp_us,
        resolution,
    )
}

fn access_unit_from_length_prefixed(
    codec: EncodedVideoCodec,
    payload: &[u8],
    nal_length_size: u8,
    timestamp_us: i64,
    resolution: VideoResolution,
) -> Result<OwnedEncodedAccessUnit, H26xParseError> {
    let nals = length_prefixed_nalus(payload, nal_length_size)?;
    access_unit_from_nalus(codec, &nals, timestamp_us, resolution)
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
fn access_unit_from_nalus(
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
fn is_keyframe_annex_b(codec: EncodedVideoCodec, bytes: &[u8]) -> Result<bool, H26xParseError> {
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

/// UUID of the `user_data_unregistered` SEI that carries an LKTS packet
/// trailer, as written by the LiveKit Go SDK and its ingest tooling.
const PACKET_TRAILER_SEI_UUID: [u8; 16] = [
    0x3f, 0xa8, 0x5f, 0x64, 0x57, 0x17, 0x45, 0x62, 0xb3, 0xfc, 0x2c, 0x96, 0x3f, 0x66, 0xaf, 0xa6,
];
const PACKET_TRAILER_MAGIC: &[u8; 4] = b"LKTS";
const PACKET_TRAILER_TAG_USER_TIMESTAMP: u8 = 0x01;
const PACKET_TRAILER_TAG_FRAME_ID: u8 = 0x02;

/// Returns the packet-trailer metadata embedded in an Annex-B access unit's
/// LKTS SEI, if it carries one.
///
/// Encoders that know a frame's capture time can prefix the access unit with
/// a `user_data_unregistered` SEI holding an LKTS packet trailer (user
/// timestamp and frame id), the convention the LiveKit Go SDK reads with
/// `ReaderTrackWithPacketTrailer`. Pass the result to
/// [`EncodedVideoPump::with_frame_metadata`](crate::encoded::EncodedVideoPump::with_frame_metadata)
/// to publish it as the frame's packet trailer. The SEI itself is left in
/// the bitstream; decoders ignore unregistered user data.
///
/// Only NAL units ahead of the first slice are inspected.
pub fn frame_metadata_from_sei(codec: EncodedVideoCodec, annex_b: &[u8]) -> Option<FrameMetadata> {
    let (sei_type, header_len) = match codec {
        EncodedVideoCodec::H264 => (6, 1),
        EncodedVideoCodec::H265 => (39, 2),
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => return None,
    };
    let (offset, prefix_len) = find_start_code(annex_b)?;
    let mut nal_start = offset + prefix_len;
    loop {
        let next = find_start_code(&annex_b[nal_start..]);
        let nal_end = next.map_or(annex_b.len(), |(offset, _)| nal_start + offset);
        let nal = &annex_b[nal_start..nal_end];
        if nal.len() > header_len {
            let nal_type = match codec {
                EncodedVideoCodec::H264 => nal[0] & 0x1f,
                _ => (nal[0] >> 1) & 0x3f,
            };
            if is_vcl_nal(codec, nal).unwrap_or(true) {
                return None;
            }
            if nal_type == sei_type {
                if let Some(metadata) = frame_metadata_from_sei_rbsp(&nal[header_len..]) {
                    return Some(metadata);
                }
            }
        }
        let (offset, prefix_len) = next?;
        nal_start += offset + prefix_len;
    }
}

/// Parses the SEI messages of one SEI NAL (after its header) for an LKTS
/// `user_data_unregistered` payload.
fn frame_metadata_from_sei_rbsp(ebsp: &[u8]) -> Option<FrameMetadata> {
    let rbsp = remove_emulation_prevention(ebsp);
    let mut i = 0;
    // more_rbsp_data(): stop at the trailing stop bit.
    while i + 1 < rbsp.len() {
        let mut payload_type = 0usize;
        while *rbsp.get(i)? == 0xff {
            payload_type += 255;
            i += 1;
        }
        payload_type += usize::from(*rbsp.get(i)?);
        i += 1;
        let mut payload_size = 0usize;
        while *rbsp.get(i)? == 0xff {
            payload_size += 255;
            i += 1;
        }
        payload_size += usize::from(*rbsp.get(i)?);
        i += 1;
        let payload = rbsp.get(i..i.checked_add(payload_size)?)?;
        i += payload_size;
        if payload_type == 5 && payload.starts_with(&PACKET_TRAILER_SEI_UUID) {
            return parse_packet_trailer(&payload[PACKET_TRAILER_SEI_UUID.len()..]);
        }
    }
    None
}

/// Parses an LKTS packet trailer: XOR-masked TLVs, then the masked trailer
/// length and the `LKTS` magic.
fn parse_packet_trailer(data: &[u8]) -> Option<FrameMetadata> {
    let magic_start = data.len().checked_sub(PACKET_TRAILER_MAGIC.len())?;
    if &data[magic_start..] != PACKET_TRAILER_MAGIC {
        return None;
    }
    let trailer_len = usize::from(*data.get(magic_start.checked_sub(1)?)? ^ 0xff);
    // Envelope: length byte plus magic.
    let envelope = 1 + PACKET_TRAILER_MAGIC.len();
    if trailer_len < envelope || trailer_len > data.len() {
        return None;
    }
    let tlvs = &data[data.len() - trailer_len..magic_start - 1];

    let mut metadata = FrameMetadata { user_timestamp: None, frame_id: None, user_data: None };
    let mut found = false;
    let mut pos = 0;
    while pos + 2 <= tlvs.len() {
        let tag = tlvs[pos] ^ 0xff;
        let len = usize::from(tlvs[pos + 1] ^ 0xff);
        pos += 2;
        let Some(value) = tlvs.get(pos..pos + len) else { break };
        let unmasked = || value.iter().fold(0u64, |acc, byte| (acc << 8) | u64::from(byte ^ 0xff));
        match (tag, len) {
            (PACKET_TRAILER_TAG_USER_TIMESTAMP, 8) => {
                metadata.user_timestamp = Some(unmasked());
                found = true;
            }
            (PACKET_TRAILER_TAG_FRAME_ID, 4) => {
                metadata.frame_id = Some(unmasked() as u32);
                found = true;
            }
            _ => {}
        }
        pos += len;
    }
    found.then_some(metadata)
}

/// Strips `emulation_prevention_three_byte`s (`00 00 03` -> `00 00`).
fn remove_emulation_prevention(ebsp: &[u8]) -> Vec<u8> {
    let mut rbsp = Vec::with_capacity(ebsp.len());
    let mut zeros = 0;
    for &byte in ebsp {
        if zeros >= 2 && byte == 0x03 {
            zeros = 0;
            continue;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        rbsp.push(byte);
    }
    rbsp
}

fn h264_nal_type(nal: &[u8]) -> Result<u8, H26xParseError> {
    let header = nal.first().ok_or(H26xParseError::EmptyPayload)?;
    Ok(header & 0x1f)
}

fn h265_nal_type(nal: &[u8]) -> Result<u8, H26xParseError> {
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

/// Returns the offset and length (3 or 4) of the first Annex-B start code.
fn find_start_code(bytes: &[u8]) -> Option<(usize, usize)> {
    // Every start code ends in 0x01 after two zero bytes; jumping between
    // 0x01 bytes and looking back is far cheaper than matching at every
    // offset of a multi-kilobyte slice.
    let mut from = 2;
    while from < bytes.len() {
        let one = from + bytes[from..].iter().position(|&byte| byte == 1)?;
        if bytes[one - 1] == 0 && bytes[one - 2] == 0 {
            return Some(if one >= 3 && bytes[one - 3] == 0 { (one - 3, 4) } else { (one - 2, 3) });
        }
        from = one + 1;
    }
    None
}

fn length_prefixed_nalus(
    payload: &[u8],
    nal_length_size: u8,
) -> Result<Vec<&[u8]>, H26xParseError> {
    let ranges = length_prefixed_nal_ranges(payload, nal_length_size, true)?;
    if ranges.is_empty() {
        return Err(H26xParseError::EmptyPayload);
    }
    Ok(ranges.into_iter().map(|range| &payload[range]).collect())
}

fn length_prefixed_nal_ranges(
    bytes: &[u8],
    nal_length_size: u8,
    at_eof: bool,
) -> Result<Vec<Range<usize>>, H26xParseError> {
    validate_nal_length_size(nal_length_size)?;

    let nal_length_size = nal_length_size as usize;
    let mut ranges = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes.len() - cursor < nal_length_size {
            if at_eof {
                return Err(H26xParseError::InvalidEncodedData("truncated NAL length prefix"));
            }
            break;
        }

        let nal_len = read_nal_length(&bytes[cursor..cursor + nal_length_size]);
        cursor += nal_length_size;
        if nal_len == 0 {
            return Err(H26xParseError::InvalidEncodedData("empty length-prefixed NAL unit"));
        }

        let Some(nal_end) = cursor.checked_add(nal_len) else {
            return Err(H26xParseError::InvalidEncodedData("NAL unit length overflow"));
        };
        if nal_end > bytes.len() {
            if at_eof {
                return Err(H26xParseError::InvalidEncodedData(
                    "truncated length-prefixed NAL unit",
                ));
            }
            break;
        }

        ranges.push(cursor..nal_end);
        cursor = nal_end;
    }

    Ok(ranges)
}

fn read_nal_length(bytes: &[u8]) -> usize {
    bytes.iter().fold(0usize, |len, byte| (len << 8) | usize::from(*byte))
}

fn validate_nal_length_size(nal_length_size: u8) -> Result<(), H26xParseError> {
    if (1..=4).contains(&nal_length_size) {
        return Ok(());
    }
    Err(H26xParseError::InvalidEncodedData("invalid NAL length prefix size"))
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

        assert_eq!(err, H26xParseError::InvalidEncodedData("truncated length-prefixed NAL unit"));
    }

    #[test]
    fn access_unit_from_hvcc_converts_length_prefixed_nals() {
        let bytes = [
            0, 0, 0, 3, 0x40, 0x01, 0x0c, // VPS
            0, 0, 0, 3, 0x42, 0x01, 0x02, // SPS
            0, 0, 0, 3, 0x44, 0x01, 0x03, // PPS
            0, 0, 0, 4, 0x26, 0x01, 0xaf, 0x08, // IDR_W_RADL
        ];
        let au = access_unit_from_h265_hvcc(&bytes, 4, 10, VideoResolution::new(640, 480)).unwrap();

        assert_eq!(au.codec, EncodedVideoCodec::H265);
        assert_eq!(au.frame_type, EncodedFrameType::Key);
        assert_eq!(
            au.payload.as_ref(),
            &[
                0, 0, 0, 1, 0x40, 0x01, 0x0c, 0, 0, 0, 1, 0x42, 0x01, 0x02, 0, 0, 0, 1, 0x44, 0x01,
                0x03, 0, 0, 0, 1, 0x26, 0x01, 0xaf, 0x08,
            ]
        );
    }

    #[test]
    fn access_unit_from_hvcc_marks_trailing_picture_as_delta() {
        let au = access_unit_from_h265_hvcc(
            &[0, 0, 0, 4, 0x02, 0x01, 0xd0, 0x09],
            4,
            10,
            VideoResolution::new(640, 480),
        )
        .unwrap();
        assert_eq!(au.frame_type, EncodedFrameType::Delta);
    }

    /// An H.265 stream laid out like streaming-tools' gstreamer-source:
    /// prefix SEI, parameter sets on keyframes, one slice, trailing AUD.
    const H265_HEV1_STREAM: [u8; 71] = [
        0, 0, 0, 3, 0x4e, 0x01, 0x05, // prefix SEI
        0, 0, 0, 3, 0x40, 0x01, 0x0c, // VPS
        0, 0, 0, 3, 0x42, 0x01, 0x02, // SPS
        0, 0, 0, 3, 0x44, 0x01, 0x03, // PPS
        0, 0, 0, 4, 0x26, 0x01, 0xaf, 0x08, // IDR_W_RADL
        0, 0, 0, 3, 0x46, 0x01, 0x50, // AUD
        0, 0, 0, 3, 0x4e, 0x01, 0x05, // prefix SEI
        0, 0, 0, 4, 0x02, 0x01, 0xd0, 0x09, // TRAIL_R
        0, 0, 0, 3, 0x46, 0x01, 0x50, // AUD
        0, 0, 0, 2, 0x4e, 0x01, // truncated SEI header (never completes)
    ];

    #[test]
    fn hvcc_parser_waits_for_next_access_unit_without_single_slice_flush() {
        let mut parser = LengthPrefixedAccessUnitParser::new(
            EncodedVideoCodec::H265,
            4,
            0,
            33_333,
            VideoResolution::new(640, 480),
        )
        .unwrap();

        // The IDR alone does not complete the access unit.
        assert!(parser.push(&H265_HEV1_STREAM[..36]).unwrap().is_none());
        // The terminating AUD does: it starts the next access unit.
        let au = parser.push(&H265_HEV1_STREAM[36..43]).unwrap().unwrap();
        assert_eq!(au.frame_type, EncodedFrameType::Key);
        assert_eq!(annex_b_nalus(&au.payload).len(), 5);
    }

    #[test]
    fn hvcc_parser_single_slice_flush_completes_at_the_slice() {
        let mut parser = LengthPrefixedAccessUnitParser::new(
            EncodedVideoCodec::H265,
            4,
            0,
            33_333,
            VideoResolution::new(640, 480),
        )
        .unwrap()
        .with_single_slice_flush(true);

        // Everything but the last byte of the IDR: nothing to publish yet.
        assert!(parser.push(&H265_HEV1_STREAM[..35]).unwrap().is_none());
        // The last byte of the slice completes the picture immediately.
        let au = parser.push(&H265_HEV1_STREAM[35..36]).unwrap().unwrap();
        assert_eq!(au.frame_type, EncodedFrameType::Key);
        assert_eq!(au.timestamp_us, 0);
        assert_eq!(
            annex_b_nalus(&au.payload).iter().map(|nal| (nal[0] >> 1) & 0x3f).collect::<Vec<_>>(),
            vec![39, 32, 33, 34, 19]
        );

        // The trailing AUD leads the next access unit.
        let au = parser.push(&H265_HEV1_STREAM[36..]).unwrap().unwrap();
        assert_eq!(au.frame_type, EncodedFrameType::Delta);
        assert_eq!(au.timestamp_us, 33_333);
        assert_eq!(
            annex_b_nalus(&au.payload).iter().map(|nal| (nal[0] >> 1) & 0x3f).collect::<Vec<_>>(),
            vec![35, 39, 1]
        );
        assert!(parser.drain().unwrap().is_none());
    }

    #[test]
    fn length_prefixed_parser_drops_trailing_aud_at_end_of_stream() {
        for single_slice_flush in [false, true] {
            let mut parser = LengthPrefixedAccessUnitParser::new(
                EncodedVideoCodec::H265,
                4,
                0,
                33_333,
                VideoResolution::new(640, 480),
            )
            .unwrap()
            .with_single_slice_flush(single_slice_flush);

            // SEI, VPS, SPS, PPS, IDR, AUD: one picture, then a lone AUD.
            let au = parser.push(&H265_HEV1_STREAM[..43]).unwrap().unwrap();
            assert_eq!(au.frame_type, EncodedFrameType::Key);
            assert!(parser.flush().unwrap().is_none());
        }
    }

    #[test]
    fn length_prefixed_parser_rejects_unsupported_codec() {
        let err = LengthPrefixedAccessUnitParser::new(
            EncodedVideoCodec::VP8,
            4,
            0,
            33_333,
            VideoResolution::new(640, 480),
        )
        .unwrap_err();
        assert_eq!(err, H26xParseError::UnsupportedCodec(EncodedVideoCodec::VP8));
    }

    /// LKTS SEI as written by streaming-tools' gstreamer-source: user
    /// timestamp 1_761_534_000_123_456, frame id 42.
    const LKTS_SEI_H265: [u8; 42] = [
        0x4e, 0x01, 0x05, 0x25, 0x3f, 0xa8, 0x5f, 0x64, 0x57, 0x17, 0x45, 0x62, 0xb3, 0xfc, 0x2c,
        0x96, 0x3f, 0x66, 0xaf, 0xa6, 0xfe, 0xf7, 0xff, 0xf9, 0xbd, 0xe4, 0xe7, 0xa9, 0xb1, 0xbf,
        0xfd, 0xfb, 0xff, 0xff, 0xff, 0xd5, 0xea, 0x4c, 0x4b, 0x54, 0x53, 0x80,
    ];

    fn annex_b(nals: &[&[u8]]) -> Vec<u8> {
        annex_b_payload(nals).unwrap()
    }

    #[test]
    fn reads_frame_metadata_from_lkts_sei() {
        let au = annex_b(&[&[0x46, 0x01, 0x50], &LKTS_SEI_H265, &[0x02, 0x01, 0xd0, 0x09]]);
        let metadata = frame_metadata_from_sei(EncodedVideoCodec::H265, &au).unwrap();
        assert_eq!(metadata.user_timestamp, Some(1_761_534_000_123_456));
        assert_eq!(metadata.frame_id, Some(42));
    }

    #[test]
    fn reads_frame_metadata_through_emulation_prevention() {
        // The masked timestamp 0x0000_ffff_fc00_0000 contains 00 00 03.
        let sei = [
            0x4e, 0x01, 0x05, 0x25, 0x3f, 0xa8, 0x5f, 0x64, 0x57, 0x17, 0x45, 0x62, 0xb3, 0xfc,
            0x2c, 0x96, 0x3f, 0x66, 0xaf, 0xa6, 0xfe, 0xf7, 0xff, 0xff, 0x00, 0x00, 0x03, 0x03,
            0xff, 0xff, 0xff, 0xfd, 0xfb, 0xff, 0xff, 0xff, 0xf8, 0xea, 0x4c, 0x4b, 0x54, 0x53,
            0x80,
        ];
        let au = annex_b(&[&sei, &[0x02, 0x01, 0xd0, 0x09]]);
        let metadata = frame_metadata_from_sei(EncodedVideoCodec::H265, &au).unwrap();
        assert_eq!(metadata.user_timestamp, Some(0x0000_ffff_fc00_0000));
        assert_eq!(metadata.frame_id, Some(7));
    }

    #[test]
    fn ignores_sei_after_the_first_slice_and_foreign_sei() {
        let slice: &[u8] = &[0x02, 0x01, 0xd0, 0x09];
        let au = annex_b(&[slice, &LKTS_SEI_H265]);
        assert!(frame_metadata_from_sei(EncodedVideoCodec::H265, &au).is_none());

        // A user_data_unregistered SEI with another UUID.
        let mut foreign = LKTS_SEI_H265;
        foreign[4] ^= 0x01;
        let au = annex_b(&[&foreign, slice]);
        assert!(frame_metadata_from_sei(EncodedVideoCodec::H265, &au).is_none());
    }

    #[test]
    fn reads_frame_metadata_from_h264_lkts_sei() {
        // Same payload behind the one-byte H.264 SEI header.
        let mut sei = vec![0x06];
        sei.extend_from_slice(&LKTS_SEI_H265[2..]);
        let au = annex_b(&[&sei, &[0x41, 0x9a, 0x02]]);
        let metadata = frame_metadata_from_sei(EncodedVideoCodec::H264, &au).unwrap();
        assert_eq!(metadata.frame_id, Some(42));
    }

    #[test]
    fn find_start_code_matches_a_scan_at_every_offset() {
        fn reference(bytes: &[u8]) -> Option<(usize, usize)> {
            (0..bytes.len()).find_map(|idx| {
                if bytes[idx..].starts_with(&[0, 0, 1]) {
                    Some((idx, 3))
                } else if bytes[idx..].starts_with(&[0, 0, 0, 1]) {
                    Some((idx, 4))
                } else {
                    None
                }
            })
        }
        // Every string of up to 8 bytes over {0, 1, 2}.
        for len in 0..=8u32 {
            for mut n in 0..3usize.pow(len) {
                let bytes: Vec<u8> = (0..len)
                    .map(|_| {
                        let byte = (n % 3) as u8;
                        n /= 3;
                        byte
                    })
                    .collect();
                assert_eq!(find_start_code(&bytes), reference(&bytes), "{bytes:?}");
            }
        }
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
        let mut parser = LengthPrefixedAccessUnitParser::new(
            EncodedVideoCodec::H264,
            4,
            100,
            33_333,
            VideoResolution::new(640, 480),
        )
        .unwrap();
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
            || {
                LengthPrefixedAccessUnitParser::new(
                    EncodedVideoCodec::H264,
                    4,
                    0,
                    33_333,
                    VideoResolution::new(640, 480),
                )
                .unwrap()
            },
            &h264_avc,
            3,
        );
    }

    #[test]
    fn chunked_length_prefixed_h265_matches_one_shot_parsing() {
        // Drop the truncated tail so the stream flushes cleanly.
        let stream = &H265_HEV1_STREAM[..65];
        for single_slice_flush in [false, true] {
            assert_chunked_matches_one_shot(
                || {
                    LengthPrefixedAccessUnitParser::new(
                        EncodedVideoCodec::H265,
                        4,
                        0,
                        33_333,
                        VideoResolution::new(640, 480),
                    )
                    .unwrap()
                    .with_single_slice_flush(single_slice_flush)
                },
                // The trailing AUD is dropped at the end of the stream.
                stream,
                2,
            );
        }
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
        let mut parser = LengthPrefixedAccessUnitParser::new(
            EncodedVideoCodec::H264,
            4,
            0,
            33_333,
            VideoResolution::new(640, 480),
        )
        .unwrap();
        let nal_len = (MAX_PENDING_ACCESS_UNIT_BYTES + 1) as u32;
        assert!(parser.push(&nal_len.to_be_bytes()).unwrap().is_none());

        let err = parser.push(&vec![0x41; MAX_PENDING_ACCESS_UNIT_BYTES]).unwrap_err();
        assert_eq!(
            err,
            H26xParseError::InvalidEncodedData("access unit exceeds maximum buffered size")
        );
    }
}
