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

pub mod h26x;
pub mod ingress;
pub mod rtp;

use bytes::Bytes;
use livekit::{
    options::VideoCodec,
    webrtc::video_frame::{
        EncodedFrameType as RtcEncodedFrameType, EncodedVideoCodec as RtcEncodedVideoCodec,
    },
};

use crate::error::CaptureError;

const ANNEX_B_START_CODE: [u8; 4] = [0, 0, 0, 1];

/// Encoded byte-stream framing used by encoded source backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncodedWireFormat {
    /// H.264 Annex-B byte stream.
    H264AnnexB,
    /// H.264/AVC byte stream with length-prefixed NAL units.
    ///
    /// `nal_length_size` is the number of big-endian length bytes before each NAL unit. Values
    /// from 1 through 4 are accepted; 4 is the common AVC configuration.
    H264Avc {
        /// Length-prefix size in bytes.
        nal_length_size: u8,
    },
    /// H.265 Annex-B byte stream.
    H265AnnexB,
    /// RTP packets for the supplied codec and RTP clock rate.
    Rtp {
        /// RTP payload codec.
        codec: EncodedVideoCodec,
        /// RTP timestamp clock rate.
        clock_rate: u32,
    },
    /// MPEG transport stream carrying encoded video.
    MpegTs,
}

/// Encoded video codec carried by an [`EncodedAccessUnit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Layer identifiers associated with an encoded frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EncodedLayerInfo {
    /// Spatial layer index, when present.
    pub spatial_id: Option<u8>,
    /// Temporal layer index, when present.
    pub temporal_id: Option<u8>,
}

/// H.264 packetization mode for passthrough metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H264PacketizationMode {
    /// Non-interleaved packetization mode.
    NonInterleaved,
}

/// Codec-specific metadata for encoded passthrough.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CodecSpecific {
    /// No codec-specific metadata.
    None,
    /// H.264-specific metadata.
    H264 {
        /// H.264 RTP packetization mode.
        packetization_mode: H264PacketizationMode,
    },
    /// H.265-specific metadata.
    H265,
    /// VP8-specific metadata.
    VP8 {
        /// Temporal layer index, when present.
        temporal_id: Option<u8>,
        /// Whether this frame synchronizes a temporal layer.
        layer_sync: bool,
    },
    /// VP9-specific metadata.
    VP9 {
        /// Temporal layer index, when present.
        temporal_id: Option<u8>,
        /// Spatial layer index, when present.
        spatial_id: Option<u8>,
        /// Whether this frame depends on an inter-layer reference.
        inter_layer_predicted: Option<bool>,
    },
    /// AV1-specific metadata.
    AV1 {
        /// RTP scalability mode, such as `L1T1`.
        scalability_mode: Option<String>,
        /// Encoded dependency descriptor bytes, when supplied by the caller.
        dependency_descriptor: Option<Vec<u8>>,
    },
}

impl Default for CodecSpecific {
    fn default() -> Self {
        Self::None
    }
}

impl CodecSpecific {
    /// Returns the single-layer default metadata for a codec, matching what
    /// the passthrough encoder synthesizes on the wire.
    pub fn default_for(codec: EncodedVideoCodec) -> Self {
        match codec {
            EncodedVideoCodec::H264 => {
                Self::H264 { packetization_mode: H264PacketizationMode::NonInterleaved }
            }
            EncodedVideoCodec::H265 => Self::H265,
            EncodedVideoCodec::VP8 => Self::VP8 { temporal_id: None, layer_sync: false },
            EncodedVideoCodec::VP9 => {
                Self::VP9 { temporal_id: None, spatial_id: None, inter_layer_predicted: None }
            }
            EncodedVideoCodec::AV1 => {
                Self::AV1 { scalability_mode: Some("L1T1".to_owned()), dependency_descriptor: None }
            }
        }
    }
}

/// Borrowed encoded payload fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodedFragment<'a> {
    /// Encoded fragment bytes.
    pub bytes: &'a [u8],
}

/// Encoded access-unit payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodedPayload<'a> {
    /// One contiguous payload buffer.
    Contiguous(&'a [u8]),
    /// Multiple payload fragments.
    Fragments(&'a [EncodedFragment<'a>]),
    /// Owned payload bytes.
    Owned(Vec<u8>),
}

impl EncodedPayload<'_> {
    pub(crate) fn is_empty(&self) -> bool {
        match self {
            Self::Contiguous(bytes) => bytes.is_empty(),
            Self::Fragments(fragments) => {
                fragments.is_empty() || fragments.iter().any(|fragment| fragment.bytes.is_empty())
            }
            Self::Owned(bytes) => bytes.is_empty(),
        }
    }

    pub(crate) fn to_vec(&self) -> Vec<u8> {
        match self {
            Self::Contiguous(bytes) => bytes.to_vec(),
            Self::Fragments(fragments) => {
                let len = fragments.iter().map(|fragment| fragment.bytes.len()).sum();
                let mut payload = Vec::with_capacity(len);
                for fragment in *fragments {
                    payload.extend_from_slice(fragment.bytes);
                }
                payload
            }
            Self::Owned(bytes) => bytes.clone(),
        }
    }
}

/// One encoded video access unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedAccessUnit<'a> {
    /// Encoded codec.
    pub codec: EncodedVideoCodec,
    /// Encoded payload.
    pub payload: EncodedPayload<'a>,
    /// Capture timestamp in microseconds.
    pub timestamp_us: i64,
    /// Encoded frame type.
    pub frame_type: EncodedFrameType,
    /// Encoded frame width in pixels.
    pub width: u32,
    /// Encoded frame height in pixels.
    pub height: u32,
    /// Optional layer identifiers.
    pub layers: EncodedLayerInfo,
    /// Optional codec-specific metadata.
    pub codec_specific: CodecSpecific,
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
    /// Encoded frame width in pixels.
    pub width: u32,
    /// Encoded frame height in pixels.
    pub height: u32,
    /// Optional layer identifiers.
    pub layers: EncodedLayerInfo,
    /// Optional codec-specific metadata.
    pub codec_specific: CodecSpecific,
}

impl OwnedEncodedAccessUnit {
    /// Creates an owned encoded access unit from contiguous bytes.
    pub fn new(
        codec: EncodedVideoCodec,
        payload: impl Into<Bytes>,
        timestamp_us: i64,
        frame_type: EncodedFrameType,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            codec,
            payload: payload.into(),
            timestamp_us,
            frame_type,
            width,
            height,
            layers: EncodedLayerInfo::default(),
            codec_specific: CodecSpecific::None,
        }
    }

    /// Borrows this owned access unit as an [`EncodedAccessUnit`].
    pub fn as_access_unit(&self) -> EncodedAccessUnit<'_> {
        EncodedAccessUnit {
            codec: self.codec,
            payload: EncodedPayload::Contiguous(&self.payload),
            timestamp_us: self.timestamp_us,
            frame_type: self.frame_type,
            width: self.width,
            height: self.height,
            layers: self.layers,
            codec_specific: self.codec_specific.clone(),
        }
    }

    /// Creates an owned access unit by copying a borrowed access unit.
    pub fn copy_from(access_unit: &EncodedAccessUnit<'_>) -> Self {
        Self {
            codec: access_unit.codec,
            payload: Bytes::from(access_unit.payload.to_vec()),
            timestamp_us: access_unit.timestamp_us,
            frame_type: access_unit.frame_type,
            width: access_unit.width,
            height: access_unit.height,
            layers: access_unit.layers,
            codec_specific: access_unit.codec_specific.clone(),
        }
    }
}

impl<'a> EncodedAccessUnit<'a> {
    /// Creates an access unit from one contiguous payload.
    pub fn contiguous(
        codec: EncodedVideoCodec,
        payload: &'a [u8],
        timestamp_us: i64,
        frame_type: EncodedFrameType,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            codec,
            payload: EncodedPayload::Contiguous(payload),
            timestamp_us,
            frame_type,
            width,
            height,
            layers: EncodedLayerInfo::default(),
            codec_specific: CodecSpecific::None,
        }
    }

    /// Creates an H.264 access unit from raw NAL-unit payloads.
    pub fn from_h264_nalus(
        nal_units: &[&[u8]],
        timestamp_us: i64,
        width: u32,
        height: u32,
    ) -> Result<EncodedAccessUnit<'static>, CaptureError> {
        Self::from_nalus(EncodedVideoCodec::H264, nal_units, timestamp_us, width, height)
    }

    /// Creates an H.265 access unit from raw NAL-unit payloads.
    pub fn from_h265_nalus(
        nal_units: &[&[u8]],
        timestamp_us: i64,
        width: u32,
        height: u32,
    ) -> Result<EncodedAccessUnit<'static>, CaptureError> {
        Self::from_nalus(EncodedVideoCodec::H265, nal_units, timestamp_us, width, height)
    }

    fn from_nalus(
        codec: EncodedVideoCodec,
        nal_units: &[&[u8]],
        timestamp_us: i64,
        width: u32,
        height: u32,
    ) -> Result<EncodedAccessUnit<'static>, CaptureError> {
        let is_key = is_keyframe_nalus(codec, nal_units)?;
        Ok(EncodedAccessUnit {
            codec,
            payload: EncodedPayload::Owned(annex_b_payload(nal_units)?),
            timestamp_us,
            frame_type: if is_key { EncodedFrameType::Key } else { EncodedFrameType::Delta },
            width,
            height,
            layers: EncodedLayerInfo::default(),
            codec_specific: CodecSpecific::default_for(codec),
        })
    }
}

/// Returns true when any NAL unit in the slice is an intra/key picture.
pub(crate) fn is_keyframe_nalus(
    codec: EncodedVideoCodec,
    nal_units: &[&[u8]],
) -> Result<bool, CaptureError> {
    match codec {
        EncodedVideoCodec::H264 => {
            nal_units.iter().try_fold(false, |is_key, nal| Ok(is_key || h264_nal_type(nal)? == 5))
        }
        EncodedVideoCodec::H265 => nal_units.iter().try_fold(false, |is_key, nal| {
            let nal_type = h265_nal_type(nal)?;
            Ok(is_key || (16..=21).contains(&nal_type))
        }),
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
    fn h264_nal_helper_assembles_annex_b_and_detects_keyframe() {
        let sps = [0x67, 1, 2, 3];
        let idr = [0x65, 4, 5, 6];
        let au = EncodedAccessUnit::from_h264_nalus(&[&sps, &idr], 10, 640, 480).unwrap();

        assert_eq!(au.codec, EncodedVideoCodec::H264);
        assert_eq!(au.frame_type, EncodedFrameType::Key);
        assert_eq!(
            au.payload,
            EncodedPayload::Owned(vec![0, 0, 0, 1, 0x67, 1, 2, 3, 0, 0, 0, 1, 0x65, 4, 5, 6])
        );
    }

    #[test]
    fn h265_nal_helper_detects_irap_keyframe() {
        let vps = [0x40, 1, 2];
        let idr_w_radl = [19 << 1, 1, 3];
        let au = EncodedAccessUnit::from_h265_nalus(&[&vps, &idr_w_radl], 10, 640, 480).unwrap();

        assert_eq!(au.codec, EncodedVideoCodec::H265);
        assert_eq!(au.frame_type, EncodedFrameType::Key);
    }

    #[test]
    fn h265_rejects_too_short_nal_header() {
        let err = EncodedAccessUnit::from_h265_nalus(&[&[0x26]], 10, 640, 480).unwrap_err();
        assert_eq!(err, CaptureError::H265NalTooShort);
    }

    #[test]
    fn fragments_reject_empty_fragment() {
        let fragments = [EncodedFragment { bytes: &[1] }, EncodedFragment { bytes: &[] }];
        let payload = EncodedPayload::Fragments(&fragments);
        assert!(payload.is_empty());
    }

    #[test]
    fn owned_access_unit_borrows_without_copying_payload() {
        let owned = OwnedEncodedAccessUnit::new(
            EncodedVideoCodec::H264,
            Bytes::from_static(&[1, 2, 3]),
            10,
            EncodedFrameType::Delta,
            640,
            480,
        );

        let borrowed = owned.as_access_unit();
        assert_eq!(borrowed.codec, EncodedVideoCodec::H264);
        assert_eq!(borrowed.payload, EncodedPayload::Contiguous(&[1, 2, 3]));
        assert_eq!(borrowed.timestamp_us, 10);
    }
}
