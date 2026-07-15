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
    options::{TrackPublishOptions, VideoEncoderBackend},
    webrtc::{
        video_frame::{EncodedVideoFrame, FrameMetadata},
        video_source::{native::NativeVideoSource, VideoResolution},
    },
};

use crate::{
    encoded::{
        CodecSpecific, EncodedAccessUnit, EncodedLayerInfo, EncodedPayload, EncodedVideoCodec,
    },
    error::CaptureError,
};

/// Additional methods for [`NativeVideoSource`] to support capture from sources.
pub trait NativeVideoSourceExt {
    /// Captures one DMA-BUF backed frame.
    ///
    /// The native capture path hands a single file descriptor to the driver
    /// and derives the plane layout from the underlying buffer itself
    /// (NvBufSurface); per-plane offsets, strides, and DRM modifiers in
    /// [`DmaBufFrame`] are informational and must describe that derived
    /// layout. Frames whose planes span multiple file descriptors or start
    /// at a nonzero offset are rejected rather than silently truncated.
    #[cfg(target_os = "linux")]
    fn capture_dmabuf(&self, frame: &DmaBufFrame) -> Result<(), CaptureError>;

    /// Captures one encoded video access unit.
    ///
    /// The passthrough path forwards single-layer streams: access units
    /// carrying temporal/spatial layer ids, an AV1 dependency descriptor, or
    /// a non-`L1T1` scalability mode are rejected so callers are not misled
    /// into thinking that metadata reaches the wire.
    fn capture_encoded(&self, access_unit: &EncodedAccessUnit<'_>) -> Result<(), CaptureError>;

    /// Captures one encoded video access unit with optional frame metadata.
    ///
    /// Metadata is only propagated to subscribers when the corresponding
    /// [`TrackPublishOptions::frame_metadata_features`] are enabled before
    /// publishing the local track.
    fn capture_encoded_with_metadata(
        &self,
        access_unit: &EncodedAccessUnit<'_>,
        frame_metadata: Option<FrameMetadata>,
    ) -> Result<(), CaptureError>;
}

impl NativeVideoSourceExt for NativeVideoSource {
    #[cfg(target_os = "linux")]
    fn capture_dmabuf(&self, frame: &DmaBufFrame) -> Result<(), CaptureError> {
        use crate::dmabuf::DmaBufFrame;
        let plane = frame.planes.first().ok_or(CaptureError::MissingDmaBufPlane)?;
        if frame.planes.iter().any(|other| other.fd != plane.fd) {
            return Err(CaptureError::UnsupportedDmaBufLayout(
                "planes must share one DMA-BUF file descriptor",
            ));
        }
        if plane.offset != 0 {
            return Err(CaptureError::UnsupportedDmaBufLayout(
                "first plane must start at offset 0",
            ));
        }
        let ok = self.source.capture_dmabuf_frame(
            plane.fd,
            frame.width,
            frame.height,
            frame.pixel_format.as_native(),
            frame.timestamp_us,
        );
        ok.then_some(()).ok_or(CaptureError::CaptureFailed)
    }

    fn capture_encoded(&self, access_unit: &EncodedAccessUnit<'_>) -> Result<(), CaptureError> {
        self.capture_encoded_with_metadata(access_unit, None)
    }

    fn capture_encoded_with_metadata(
        &self,
        access_unit: &EncodedAccessUnit<'_>,
        frame_metadata: Option<FrameMetadata>,
    ) -> Result<(), CaptureError> {
        validate_encoded_access_unit(access_unit)?;

        let scratch;
        let payload: &[u8] = match &access_unit.payload {
            EncodedPayload::Contiguous(bytes) => bytes,
            EncodedPayload::Owned(bytes) => bytes,
            EncodedPayload::Fragments(_) => {
                scratch = access_unit.payload.to_vec();
                &scratch
            }
        };
        let frame = EncodedVideoFrame {
            codec: access_unit.codec.into(),
            payload,
            timestamp_us: access_unit.timestamp_us,
            frame_type: access_unit.frame_type.into(),
            resolution: VideoResolution { width: access_unit.width, height: access_unit.height },
            frame_metadata,
        };
        self.capture_encoded_frame(&frame).then_some(()).ok_or(CaptureError::CaptureFailed)
    }
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

fn validate_encoded_access_unit(access_unit: &EncodedAccessUnit<'_>) -> Result<(), CaptureError> {
    if access_unit.payload.is_empty() {
        return Err(CaptureError::EmptyPayload);
    }
    if access_unit.layers != EncodedLayerInfo::default() {
        return Err(CaptureError::UnsupportedLayeredEncoding(
            "temporal/spatial layer ids are not forwarded by the passthrough encoder",
        ));
    }
    let default_specific = CodecSpecific::default_for(access_unit.codec);
    if access_unit.codec_specific != CodecSpecific::None
        && access_unit.codec_specific != default_specific
    {
        return Err(CaptureError::UnsupportedLayeredEncoding(
            "codec-specific layering metadata is not forwarded by the passthrough encoder",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoded::EncodedFrameType;

    #[test]
    fn accepts_vp8_vp9_and_av1_access_units() {
        for codec in [EncodedVideoCodec::VP8, EncodedVideoCodec::VP9, EncodedVideoCodec::AV1] {
            let access_unit = EncodedAccessUnit::contiguous(
                codec,
                &[1, 2, 3],
                0,
                EncodedFrameType::Key,
                640,
                480,
            );

            assert!(validate_encoded_access_unit(&access_unit).is_ok());
        }
    }

    #[test]
    fn rejects_empty_encoded_access_units() {
        let access_unit = EncodedAccessUnit::contiguous(
            EncodedVideoCodec::VP8,
            &[],
            0,
            EncodedFrameType::Key,
            640,
            480,
        );

        assert_eq!(validate_encoded_access_unit(&access_unit), Err(CaptureError::EmptyPayload));
    }

    #[test]
    fn accepts_default_codec_specific_metadata() {
        let mut access_unit = EncodedAccessUnit::contiguous(
            EncodedVideoCodec::AV1,
            &[1, 2, 3],
            0,
            EncodedFrameType::Key,
            640,
            480,
        );
        access_unit.codec_specific = CodecSpecific::default_for(EncodedVideoCodec::AV1);

        assert!(validate_encoded_access_unit(&access_unit).is_ok());
    }

    #[test]
    fn rejects_layered_access_units() {
        let mut access_unit = EncodedAccessUnit::contiguous(
            EncodedVideoCodec::VP9,
            &[1, 2, 3],
            0,
            EncodedFrameType::Key,
            640,
            480,
        );
        access_unit.layers = EncodedLayerInfo { spatial_id: None, temporal_id: Some(1) };

        assert!(matches!(
            validate_encoded_access_unit(&access_unit),
            Err(CaptureError::UnsupportedLayeredEncoding(_))
        ));
    }

    #[test]
    fn rejects_non_default_codec_specific_metadata() {
        let mut access_unit = EncodedAccessUnit::contiguous(
            EncodedVideoCodec::VP8,
            &[1, 2, 3],
            0,
            EncodedFrameType::Key,
            640,
            480,
        );
        access_unit.codec_specific = CodecSpecific::VP8 { temporal_id: Some(1), layer_sync: true };

        assert!(matches!(
            validate_encoded_access_unit(&access_unit),
            Err(CaptureError::UnsupportedLayeredEncoding(_))
        ));
    }
}
