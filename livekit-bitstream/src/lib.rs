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

//! **Internal crate.** Used by the LiveKit Rust SDK; not intended for direct use.
//! No API stability guarantees: any release may contain breaking changes.
//!
//! H.264/H.265 Annex-B NAL unit splitting, NAL types, and keyframe detection.

#![forbid(unsafe_code)]

use std::ops::Range;
use thiserror::Error;

/// Error returned when a NAL unit is too short to read its header.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum NalError {
    /// NAL unit is empty.
    #[error("NAL unit is empty")]
    Empty,
    /// H.265 NAL unit is too short to contain its two-byte header.
    #[error("H.265 NAL unit is too short")]
    H265TooShort,
}

/// Returns the offset and length of the first Annex-B start code
/// (`00 00 01` or `00 00 00 01`) in `bytes`.
pub fn find_start_code(bytes: &[u8]) -> Option<(usize, usize)> {
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

/// Returns NAL unit byte ranges, excluding start codes, for an Annex-B
/// access unit or stream chunk.
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

/// Returns the non-empty NAL units in an Annex-B buffer.
///
/// ```
/// let bytes = [0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x65, 2, 3];
/// let nals = livekit_bitstream::annex_b_nalus(&bytes);
/// assert_eq!(nals, vec![&[0x67, 1][..], &[0x65, 2, 3][..]]);
/// ```
pub fn annex_b_nalus(bytes: &[u8]) -> Vec<&[u8]> {
    annex_b_nal_ranges(bytes)
        .into_iter()
        .map(|range| &bytes[range])
        .filter(|nal| !nal.is_empty())
        .collect()
}

/// Returns the `nal_unit_type` of an H.264 NAL unit.
pub fn h264_nal_type(nal: &[u8]) -> Result<u8, NalError> {
    let header = nal.first().ok_or(NalError::Empty)?;
    Ok(header & 0x1f)
}

/// Returns the `nal_unit_type` of an H.265 NAL unit.
pub fn h265_nal_type(nal: &[u8]) -> Result<u8, NalError> {
    if nal.is_empty() {
        return Err(NalError::Empty);
    }
    if nal.len() < 2 {
        return Err(NalError::H265TooShort);
    }
    Ok((nal[0] >> 1) & 0x3f)
}

/// Returns `true` when the H.264 NAL units contain an IDR picture.
pub fn h264_is_keyframe(nal_units: &[&[u8]]) -> Result<bool, NalError> {
    nal_units.iter().try_fold(false, |is_key, nal| Ok(is_key || h264_nal_type(nal)? == 5))
}

/// Returns `true` when the H.265 NAL units form a WebRTC-usable keyframe:
/// parameter sets (VPS/SPS/PPS) plus an IDR picture.
pub fn h265_is_keyframe(nal_units: &[&[u8]]) -> Result<bool, NalError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h264_keyframe_requires_idr_nal() {
        let sps = [0x67, 1, 2, 3];
        let idr = [0x65, 4, 5, 6];
        let non_idr = [0x61, 1, 2];

        assert!(h264_is_keyframe(&[&sps, &idr]).unwrap());
        assert!(!h264_is_keyframe(&[&sps, &non_idr]).unwrap());
    }

    #[test]
    fn h265_keyframe_requires_parameter_sets_and_idr() {
        let vps = [0x40, 1, 2];
        let sps = [0x42, 1, 2];
        let pps = [0x44, 1, 2];
        let idr_w_radl = [19 << 1, 1, 3];
        let cra = [21 << 1, 1, 3];

        assert!(!h265_is_keyframe(&[&vps, &idr_w_radl]).unwrap());
        assert!(h265_is_keyframe(&[&vps, &sps, &pps, &idr_w_radl]).unwrap());
        assert!(!h265_is_keyframe(&[&vps, &sps, &pps, &cra]).unwrap());
    }

    #[test]
    fn nal_type_rejects_short_headers() {
        assert_eq!(h264_nal_type(&[]).unwrap_err(), NalError::Empty);
        assert_eq!(h265_nal_type(&[]).unwrap_err(), NalError::Empty);
        assert_eq!(h265_nal_type(&[0x26]).unwrap_err(), NalError::H265TooShort);
    }

    #[test]
    fn nal_ranges_skip_empty_units_between_start_codes() {
        let bytes = [0, 0, 1, 0, 0, 1, 0x65, 1];
        assert_eq!(annex_b_nal_ranges(&bytes), vec![6..8]);
    }
}
