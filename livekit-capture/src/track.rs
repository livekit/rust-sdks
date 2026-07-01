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
    prelude::LocalVideoTrack,
    webrtc::{
        video_frame::{EncodedVideoFrame, VideoBuffer, VideoFrame},
        video_source::{native::NativeVideoSource, RtcVideoSource, VideoResolution},
    },
};

use crate::{
    encoded::{EncodedAccessUnit, EncodedVideoCodec},
    error::CaptureError,
};

pub use crate::device::CapturePath;
#[cfg(target_os = "linux")]
use crate::dmabuf::DmaBufFrame;

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
        let ok = self.source.capture_dmabuf_frame(
            plane.fd,
            frame.width,
            frame.height,
            frame.pixel_format.as_native(),
            frame.timestamp_us,
        );
        ok.then_some(()).ok_or(CaptureError::CaptureFailed)
    }

    /// Captures one encoded video access unit.
    pub fn capture_encoded(&self, access_unit: &EncodedAccessUnit<'_>) -> Result<(), CaptureError> {
        validate_encoded_access_unit(access_unit)?;

        let payload = access_unit.payload.to_vec();
        let frame = EncodedVideoFrame {
            codec: access_unit.codec.into(),
            payload: &payload,
            timestamp_us: access_unit.timestamp_us,
            frame_type: access_unit.frame_type.into(),
            width: access_unit.width,
            height: access_unit.height,
            frame_metadata: None,
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

fn validate_encoded_access_unit(access_unit: &EncodedAccessUnit<'_>) -> Result<(), CaptureError> {
    if access_unit.payload.is_empty() {
        return Err(CaptureError::EmptyPayload);
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
}
