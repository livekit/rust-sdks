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

//! Encoded video: codec vocabulary, access units, the source contract for
//! pre-encoded ingest, and the pump.
//!
//! Sources produce crate-owned access units — the vocabulary the parsing
//! and validation helpers speak — and [`EncodedVideoPump`] bridges them into
//! an RTC track as passthrough. The source trait is object-safe and
//! implemented for `Box<dyn ...>`, so sources can be constructed dynamically
//! and driven through the same generic pump.

use crate::{
    error::{CaptureError, SourceError},
    primitive::VideoResolution,
    pump::PumpStop,
};
use bytes::Bytes;
use livekit::{
    options::VideoCodec,
    webrtc::{
        video_frame::{
            EncodedFrameType as RtcEncodedFrameType, EncodedVideoCodec as RtcEncodedVideoCodec,
        },
        video_source::EncodedRateControl,
    },
};

pub mod h26x;
mod pump;
pub use pump::EncodedVideoPump;

const ANNEX_B_START_CODE: [u8; 4] = [0, 0, 0, 1];

/// Source of pre-encoded video access units, such as an encoding pipeline.
pub trait EncodedVideoSource: Send {
    /// Nominal output resolution, used to size the RTC source.
    fn resolution(&self) -> VideoResolution;

    /// Codec produced by this source; fixed for the source's lifetime.
    fn codec(&self) -> EncodedVideoCodec;

    /// Blocks until the next access unit is available, returning `Ok(None)`
    /// when the source reaches the end of its stream.
    ///
    /// Sources must return promptly (with `Ok(None)`) once `stop` fires:
    /// integrate it into the blocking wait, or bound each wait so the token
    /// is observed within a frame interval or so. The pump distinguishes a
    /// stop from end of stream via the token.
    fn next_access_unit(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<OwnedEncodedAccessUnit>, SourceError>;

    /// Forwards a downstream keyframe request (PLI/FIR, late subscriber) to
    /// the producer so it can emit an IDR.
    ///
    /// The default implementation does nothing, for transports that cannot
    /// influence the upstream encoder.
    fn request_keyframe(&mut self) {}

    /// Forwards a downstream rate-control target to the producer.
    ///
    /// The default implementation does nothing, for transports that cannot
    /// influence the upstream encoder.
    fn update_rate_control(&mut self, _target: EncodedRateControl) {}
}

/// Encoded video codec carried by an [`OwnedEncodedAccessUnit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub enum EncodedVideoCodec {
    /// H.264/AVC video.
    H264,
    /// H.265/HEVC video.
    H265,
    /// VP8 video.
    VP8,
    /// VP9 video.
    VP9,
    /// AV1 video.
    AV1,
}

/// Encoded video frame type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodedFrameType {
    /// A key frame.
    Key,
    /// A delta frame.
    Delta,
}

/// Owned encoded video access unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedEncodedAccessUnit {
    /// Encoded codec.
    pub codec: EncodedVideoCodec,
    /// Encoded payload bytes.
    pub payload: Bytes,
    /// Capture timestamp in microseconds.
    pub timestamp_us: i64,
    /// Encoded frame type.
    pub frame_type: EncodedFrameType,
    /// Encoded frame resolution in pixels.
    pub resolution: VideoResolution,
}

impl OwnedEncodedAccessUnit {
    /// Creates an owned encoded access unit from contiguous bytes.
    pub fn new(
        codec: EncodedVideoCodec,
        payload: impl Into<Bytes>,
        timestamp_us: i64,
        frame_type: EncodedFrameType,
        resolution: VideoResolution,
    ) -> Self {
        Self { codec, payload: payload.into(), timestamp_us, frame_type, resolution }
    }
}

/// Returns true when the NAL units form a WebRTC-usable key frame.
pub(crate) fn is_keyframe_nalus(
    codec: EncodedVideoCodec,
    nal_units: &[&[u8]],
) -> Result<bool, CaptureError> {
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
            Err(CaptureError::UnsupportedCodec(codec))
        }
    }
}

impl From<EncodedVideoCodec> for VideoCodec {
    fn from(value: EncodedVideoCodec) -> Self {
        match value {
            EncodedVideoCodec::H264 => Self::H264,
            EncodedVideoCodec::H265 => Self::H265,
            EncodedVideoCodec::VP8 => Self::VP8,
            EncodedVideoCodec::VP9 => Self::VP9,
            EncodedVideoCodec::AV1 => Self::AV1,
        }
    }
}

impl From<EncodedVideoCodec> for RtcEncodedVideoCodec {
    fn from(value: EncodedVideoCodec) -> Self {
        match value {
            EncodedVideoCodec::H264 => Self::H264,
            EncodedVideoCodec::H265 => Self::H265,
            EncodedVideoCodec::VP8 => Self::VP8,
            EncodedVideoCodec::VP9 => Self::VP9,
            EncodedVideoCodec::AV1 => Self::AV1,
        }
    }
}

impl From<EncodedFrameType> for RtcEncodedFrameType {
    fn from(value: EncodedFrameType) -> Self {
        match value {
            EncodedFrameType::Key => Self::Key,
            EncodedFrameType::Delta => Self::Delta,
        }
    }
}

impl<S: EncodedVideoSource + ?Sized> EncodedVideoSource for Box<S> {
    fn resolution(&self) -> VideoResolution {
        (**self).resolution()
    }

    fn codec(&self) -> EncodedVideoCodec {
        (**self).codec()
    }

    fn next_access_unit(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<OwnedEncodedAccessUnit>, SourceError> {
        (**self).next_access_unit(stop)
    }

    fn request_keyframe(&mut self) {
        (**self).request_keyframe()
    }

    fn update_rate_control(&mut self, target: EncodedRateControl) {
        (**self).update_rate_control(target)
    }
}

// Object safety is part of this trait's contract: dynamic applications box
// sources at their edge and drive them through the same generic pumps.
const _: () = {
    fn _assert_object_safe(_: &dyn EncodedVideoSource) {}
};

pub(crate) fn h264_nal_type(nal: &[u8]) -> Result<u8, CaptureError> {
    let header = nal.first().ok_or(CaptureError::EmptyPayload)?;
    Ok(header & 0x1f)
}

pub(crate) fn h265_nal_type(nal: &[u8]) -> Result<u8, CaptureError> {
    if nal.is_empty() {
        return Err(CaptureError::EmptyPayload);
    }
    if nal.len() < 2 {
        return Err(CaptureError::H265NalTooShort);
    }
    Ok((nal[0] >> 1) & 0x3f)
}

pub(crate) fn annex_b_payload(nal_units: &[&[u8]]) -> Result<Vec<u8>, CaptureError> {
    if nal_units.is_empty() {
        return Err(CaptureError::EmptyPayload);
    }
    let len = nal_units.iter().try_fold(0usize, |len, nal| {
        if nal.is_empty() {
            Err(CaptureError::EmptyPayload)
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
        assert_eq!(err, CaptureError::H265NalTooShort);
    }

    #[test]
    fn annex_b_payload_prefixes_each_nal_unit() {
        let payload = annex_b_payload(&[&[0x67, 1, 2, 3], &[0x65, 4, 5, 6]]).unwrap();
        assert_eq!(payload, vec![0, 0, 0, 1, 0x67, 1, 2, 3, 0, 0, 0, 1, 0x65, 4, 5, 6]);
    }

    #[test]
    fn annex_b_payload_rejects_empty_input() {
        assert_eq!(annex_b_payload(&[]).unwrap_err(), CaptureError::EmptyPayload);
        assert_eq!(annex_b_payload(&[&[]]).unwrap_err(), CaptureError::EmptyPayload);
    }
}
