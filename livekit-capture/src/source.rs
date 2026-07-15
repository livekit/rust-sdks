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

use std::{error::Error, fmt};

use livekit::webrtc::{
    video_frame::{native::NativeBuffer, I420Buffer, VideoFrame},
    video_source::native::NativeVideoSource,
};

use crate::{
    device::{CaptureFormat, CaptureFrameFormat, CapturePath},
    dmabuf::DmaBufFrame,
    encoded::{ingress::EncodedAccessUnitSource, OwnedEncodedAccessUnit},
    error::CaptureError,
    track::NativeVideoSourceExt,
};

/// Uncompressed CPU-accessible video frame buffer produced by a capture source.
#[derive(Debug)]
pub struct RawVideoFrame {
    /// I420 video frame suitable for [`VideoCaptureTrack::capture_frame`].
    pub frame: VideoFrame<I420Buffer>,
    /// Source format delivered by the capture backend before conversion to I420.
    pub source_format: CaptureFrameFormat,
    /// Wall-clock capture timestamp in microseconds.
    pub capture_wall_time_us: u64,
    /// Wall-clock timestamp recorded after the frame was read, in microseconds.
    pub read_wall_time_us: u64,
    /// Backend capture timestamp translated to UNIX-epoch microseconds, when available.
    pub sensor_timestamp_us: Option<u64>,
    /// Whether the backend converted the source buffer before publishing.
    pub used_conversion: bool,
}

/// Platform-native uncompressed video frame buffer produced by a capture source.
#[derive(Debug)]
pub struct NativeVideoFrame {
    /// Native video frame suitable for [`VideoCaptureTrack::capture_frame`].
    pub frame: VideoFrame<NativeBuffer>,
    /// Source format delivered by the capture backend.
    pub source_format: CaptureFrameFormat,
    /// Wall-clock capture timestamp in microseconds.
    pub capture_wall_time_us: u64,
    /// Wall-clock timestamp recorded after the frame was read, in microseconds.
    pub read_wall_time_us: u64,
    /// Backend capture timestamp translated to UNIX-epoch microseconds, when available.
    pub sensor_timestamp_us: Option<u64>,
}

/// Frame produced by a capture source.
#[derive(Debug)]
#[non_exhaustive]
pub enum CaptureFrame {
    /// Platform-native uncompressed frame.
    Native(NativeVideoFrame),
    /// Uncompressed CPU-accessible frame.
    Raw(RawVideoFrame),
    /// Linux DMA-BUF backed frame.
    DmaBuf(DmaBufFrame),
    /// Encoded video access unit.
    Encoded(OwnedEncodedAccessUnit),
}

impl CaptureFrame {
    /// Returns the capture path used by this frame.
    pub fn capture_path(&self) -> CapturePath {
        match self {
            Self::Native(_) => CapturePath::Native,
            Self::Raw(_) => CapturePath::Raw,
            Self::DmaBuf(_) => CapturePath::DmaBuf,
            Self::Encoded(_) => CapturePath::Encoded,
        }
    }
}

impl CaptureFrame {
    /// Publishes this frame into a LiveKit capture track.
    pub fn capture_to(&self, source: &NativeVideoSource) -> Result<(), CaptureError> {
        match self {
            Self::Native(frame) => {
                source.capture_frame(&frame.frame);
                Ok(())
            }
            Self::Raw(frame) => {
                source.capture_frame(&frame.frame);
                Ok(())
            }
            #[cfg(target_os = "linux")]
            Self::DmaBuf(frame) => source.capture_dmabuf(frame),
            #[cfg(not(target_os = "linux"))]
            Self::DmaBuf(_) => Err(CaptureError::UnsupportedPlatform("DMA-BUF capture")),
            Self::Encoded(access_unit) => source.capture_encoded(&access_unit.as_access_unit()),
        }
    }
}

/// Source that produces one of the common capture frame paths.
pub trait CaptureFrameSource {
    /// Error returned by the source.
    type Error: Error + Send + Sync + 'static;

    /// Returns the capture path produced by this source.
    fn capture_path(&self) -> CapturePath;

    /// Returns the negotiated capture format when the source has one.
    fn format(&self) -> Option<CaptureFormat>;

    /// Captures the next frame.
    fn next_frame(&mut self) -> Result<CaptureFrame, Self::Error>;
}

/// Adapts an [`EncodedAccessUnitSource`] into the common frame-source model.
#[derive(Debug)]
pub struct EncodedCaptureFrameSource<S> {
    source: S,
}

impl<S> EncodedCaptureFrameSource<S> {
    /// Creates a frame-source adapter for an encoded access-unit source.
    pub fn new(source: S) -> Self {
        Self { source }
    }

    /// Returns the underlying encoded source.
    pub fn source(&self) -> &S {
        &self.source
    }

    /// Returns the underlying encoded source mutably.
    pub fn source_mut(&mut self) -> &mut S {
        &mut self.source
    }

    /// Consumes this adapter and returns the underlying encoded source.
    pub fn into_inner(self) -> S {
        self.source
    }
}

impl<S> CaptureFrameSource for EncodedCaptureFrameSource<S>
where
    S: EncodedAccessUnitSource,
{
    type Error = EncodedFrameSourceError<S::Error>;

    fn capture_path(&self) -> CapturePath {
        CapturePath::Encoded
    }

    fn format(&self) -> Option<CaptureFormat> {
        None
    }

    fn next_frame(&mut self) -> Result<CaptureFrame, Self::Error> {
        let Some(access_unit) =
            self.source.next_access_unit().map_err(EncodedFrameSourceError::Source)?
        else {
            return Err(EncodedFrameSourceError::EndOfStream);
        };
        Ok(CaptureFrame::Encoded(access_unit))
    }
}

/// Error returned by [`EncodedCaptureFrameSource`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodedFrameSourceError<E> {
    /// The encoded source reached EOF.
    EndOfStream,
    /// The encoded source failed.
    Source(E),
}

impl<E: fmt::Display> fmt::Display for EncodedFrameSourceError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EndOfStream => f.write_str("encoded source reached end of stream"),
            Self::Source(err) => write!(f, "encoded source failed: {err}"),
        }
    }
}

impl<E> Error for EncodedFrameSourceError<E>
where
    E: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::EndOfStream => None,
            Self::Source(err) => Some(err),
        }
    }
}
