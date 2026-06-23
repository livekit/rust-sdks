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

use livekit::{
    options::{TrackPublishOptions, VideoCodec, VideoEncoderBackend},
    prelude::LocalVideoTrack,
    webrtc::{
        video_frame::{
            EncodedFrameType as RtcEncodedFrameType, EncodedVideoCodec as RtcEncodedVideoCodec,
            EncodedVideoFrame, FrameMetadata, VideoBuffer, VideoFrame,
        },
        video_source::{native::NativeVideoSource, RtcVideoSource, VideoResolution},
    },
};
use thiserror::Error;

const ANNEX_B_START_CODE: [u8; 4] = [0, 0, 0, 1];

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

/// Packet-trailer metadata associated with an encoded frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EncodedFrameMetadata {
    /// Wall-clock capture timestamp in microseconds.
    pub user_timestamp: Option<u64>,
    /// Monotonically increasing frame identifier.
    pub frame_id: Option<u32>,
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
    fn is_empty(&self) -> bool {
        match self {
            Self::Contiguous(bytes) => bytes.is_empty(),
            Self::Fragments(fragments) => {
                fragments.is_empty() || fragments.iter().any(|fragment| fragment.bytes.is_empty())
            }
            Self::Owned(bytes) => bytes.is_empty(),
        }
    }

    fn to_vec(&self) -> Vec<u8> {
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
    /// Optional packet-trailer metadata.
    pub metadata: EncodedFrameMetadata,
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
            metadata: EncodedFrameMetadata::default(),
        }
    }

    /// Creates an H.264 access unit from raw NAL-unit payloads.
    pub fn from_h264_nalus(
        nal_units: &[&[u8]],
        timestamp_us: i64,
        width: u32,
        height: u32,
    ) -> Result<EncodedAccessUnit<'static>, CaptureError> {
        let mut is_key = false;
        for nal in nal_units {
            let nal_type = h264_nal_type(nal)?;
            if nal_type == 5 {
                is_key = true;
            }
        }

        Ok(EncodedAccessUnit {
            codec: EncodedVideoCodec::H264,
            payload: EncodedPayload::Owned(annex_b_payload(nal_units)?),
            timestamp_us,
            frame_type: if is_key { EncodedFrameType::Key } else { EncodedFrameType::Delta },
            width,
            height,
            layers: EncodedLayerInfo::default(),
            codec_specific: CodecSpecific::H264 {
                packetization_mode: H264PacketizationMode::NonInterleaved,
            },
            metadata: EncodedFrameMetadata::default(),
        })
    }

    /// Creates an H.265 access unit from raw NAL-unit payloads.
    pub fn from_h265_nalus(
        nal_units: &[&[u8]],
        timestamp_us: i64,
        width: u32,
        height: u32,
    ) -> Result<EncodedAccessUnit<'static>, CaptureError> {
        let mut is_key = false;
        for nal in nal_units {
            let nal_type = h265_nal_type(nal)?;
            if (16..=21).contains(&nal_type) {
                is_key = true;
            }
        }

        Ok(EncodedAccessUnit {
            codec: EncodedVideoCodec::H265,
            payload: EncodedPayload::Owned(annex_b_payload(nal_units)?),
            timestamp_us,
            frame_type: if is_key { EncodedFrameType::Key } else { EncodedFrameType::Delta },
            width,
            height,
            layers: EncodedLayerInfo::default(),
            codec_specific: CodecSpecific::H265,
            metadata: EncodedFrameMetadata::default(),
        })
    }
}

/// DMA-BUF pixel format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaBufPixelFormat {
    /// NV12 biplanar format.
    Nv12,
    /// YUV420M multiplanar format.
    Yuv420M,
}

impl DmaBufPixelFormat {
    #[cfg(target_os = "linux")]
    fn as_native(self) -> i32 {
        match self {
            Self::Nv12 => 0,
            Self::Yuv420M => 1,
        }
    }
}

/// One DMA-BUF plane descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaBufPlane {
    /// DMA-BUF file descriptor.
    pub fd: i32,
    /// Plane byte offset.
    pub offset: u32,
    /// Plane byte stride.
    pub stride: u32,
}

/// One DMA-BUF backed captured frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmaBufFrame {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Pixel format.
    pub pixel_format: DmaBufPixelFormat,
    /// DMA-BUF planes.
    pub planes: Vec<DmaBufPlane>,
    /// Optional DRM format modifier.
    pub modifier: Option<u64>,
    /// Capture timestamp in microseconds.
    pub timestamp_us: i64,
    /// Optional packet-trailer metadata.
    pub metadata: EncodedFrameMetadata,
}

/// Capture source backed by a LiveKit local video track.
#[derive(Debug, Clone)]
pub struct VideoCaptureTrack {
    source: NativeVideoSource,
    track: LocalVideoTrack,
}

impl VideoCaptureTrack {
    /// Creates a capture track with the supplied resolution.
    pub fn new(name: &str, resolution: VideoResolution, is_screencast: bool) -> Self {
        let source = NativeVideoSource::new(resolution, is_screencast);
        let track =
            LocalVideoTrack::create_video_track(name, RtcVideoSource::Native(source.clone()));
        Self { source, track }
    }

    /// Returns the publishable local video track.
    pub fn track(&self) -> LocalVideoTrack {
        self.track.clone()
    }

    /// Captures one decoded video frame.
    pub fn capture_frame<T: AsRef<dyn VideoBuffer>>(&self, frame: &VideoFrame<T>) {
        self.source.capture_frame(frame);
    }

    /// Captures one DMA-BUF backed frame.
    #[cfg(target_os = "linux")]
    pub fn capture_dmabuf(&self, frame: &DmaBufFrame) -> Result<(), CaptureError> {
        let plane = frame.planes.first().ok_or(CaptureError::MissingDmaBufPlane)?;
        let ok = self.source.capture_dmabuf_frame_with_metadata(
            plane.fd,
            frame.width,
            frame.height,
            frame.pixel_format.as_native(),
            frame.timestamp_us,
            frame.metadata.into_frame_metadata(),
        );
        ok.then_some(()).ok_or(CaptureError::CaptureFailed)
    }

    /// Captures one encoded video access unit.
    pub fn capture_encoded(&self, access_unit: &EncodedAccessUnit<'_>) -> Result<(), CaptureError> {
        match access_unit.codec {
            EncodedVideoCodec::H264 | EncodedVideoCodec::H265 => {}
            EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
                return Err(CaptureError::UnsupportedCodec(access_unit.codec));
            }
        }
        if access_unit.payload.is_empty() {
            return Err(CaptureError::EmptyPayload);
        }

        let payload = access_unit.payload.to_vec();
        let frame = EncodedVideoFrame {
            codec: access_unit.codec.into(),
            payload: &payload,
            timestamp_us: access_unit.timestamp_us,
            frame_type: access_unit.frame_type.into(),
            width: access_unit.width,
            height: access_unit.height,
            frame_metadata: access_unit.metadata.into_frame_metadata(),
        };
        self.source.capture_encoded_frame(&frame).then_some(()).ok_or(CaptureError::CaptureFailed)
    }

    /// Returns publish options appropriate for encoded passthrough.
    pub fn encoded_publish_options(codec: EncodedVideoCodec) -> TrackPublishOptions {
        TrackPublishOptions {
            video_codec: codec.into(),
            video_encoder: VideoEncoderBackend::PreEncoded,
            simulcast: false,
            ..Default::default()
        }
    }
}

/// Error returned by capture helpers.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CaptureError {
    /// Encoded payload is empty.
    #[error("encoded payload is empty")]
    EmptyPayload,
    /// H.265 NAL unit is too short to contain its header.
    #[error("H.265 NAL unit is too short")]
    H265NalTooShort,
    /// DMA-BUF frame did not include any planes.
    #[error("DMA-BUF frame did not include any planes")]
    MissingDmaBufPlane,
    /// Codec is represented by the API but not yet supported by native passthrough.
    #[error("encoded passthrough does not support {0:?} yet")]
    UnsupportedCodec(EncodedVideoCodec),
    /// The underlying source rejected the frame.
    #[error("capture source rejected the frame")]
    CaptureFailed,
}

impl EncodedFrameMetadata {
    fn into_frame_metadata(self) -> Option<FrameMetadata> {
        (self.user_timestamp.is_some() || self.frame_id.is_some()).then_some(FrameMetadata {
            user_timestamp: self.user_timestamp,
            frame_id: self.frame_id,
        })
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

fn h264_nal_type(nal: &[u8]) -> Result<u8, CaptureError> {
    let header = nal.first().ok_or(CaptureError::EmptyPayload)?;
    Ok(header & 0x1f)
}

fn h265_nal_type(nal: &[u8]) -> Result<u8, CaptureError> {
    if nal.is_empty() {
        return Err(CaptureError::EmptyPayload);
    }
    if nal.len() < 2 {
        return Err(CaptureError::H265NalTooShort);
    }
    Ok((nal[0] >> 1) & 0x3f)
}

fn annex_b_payload(nal_units: &[&[u8]]) -> Result<Vec<u8>, CaptureError> {
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
}
