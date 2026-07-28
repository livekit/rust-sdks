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

//! Video capture source traits and types.
//!
//! Everything in this module is independent of libwebrtc: sources produce
//! crate-owned frame types and receive crate-owned feedback types. The pumps
//! in [`pump`](crate::pump) bridge a source into an RTC track and mediate all
//! communication with the WebRTC stack.
//!
//! Both source traits are object-safe, and `Box<dyn ...>` boxes implement
//! them, so applications that construct sources dynamically can drive a
//! [`PixelPump<Box<dyn PixelVideoSource>>`](crate::pump::PixelPump) while
//! applications that know their source statically pay for no type erasure.

use std::{error::Error, fmt};

use bytes::Bytes;

use crate::encoded::{EncodedVideoCodec, OwnedEncodedAccessUnit};

/// Video resolution in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoResolution {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
}

/// Encoder rate-control target forwarded from WebRTC to an encoded source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RateControl {
    /// Target bitrate in bits per second.
    pub target_bitrate_bps: u64,
    /// Target frame rate in frames per second.
    pub framerate_fps: f64,
}

/// Error returned by a capture source.
///
/// Backend-specific errors are type-erased so sources stay usable as trait
/// objects; the wrapped error remains reachable for display and through
/// [`Error::source`].
#[derive(Debug)]
pub struct SourceError(Box<dyn Error + Send + Sync>);

impl SourceError {
    /// Wraps a backend error.
    pub fn new(error: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self(error.into())
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl Error for SourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}

/// Pixel data of one video frame.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PixelVideoData {
    /// Planar YUV 4:2:0 with 8-bit samples.
    I420 {
        /// Luma plane.
        y: Bytes,
        /// Blue-difference chroma plane.
        u: Bytes,
        /// Red-difference chroma plane.
        v: Bytes,
        /// Luma plane stride in bytes.
        stride_y: u32,
        /// U plane stride in bytes.
        stride_u: u32,
        /// V plane stride in bytes.
        stride_v: u32,
    },
}

/// One pixel video frame produced by a [`PixelVideoSource`].
#[derive(Debug, Clone)]
pub struct PixelVideoFrame {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Capture timestamp in microseconds.
    pub timestamp_us: i64,
    /// Pixel data.
    pub data: PixelVideoData,
}

/// Source of pixel (unencoded) video frames, such as a camera device.
pub trait PixelVideoSource: Send {
    /// Nominal output resolution, used to size the RTC source.
    fn resolution(&self) -> VideoResolution;

    /// Blocks until the next frame is available, returning `Ok(None)` when
    /// the source reaches the end of its stream.
    fn next_frame(&mut self) -> Result<Option<PixelVideoFrame>, SourceError>;
}

/// Source of pre-encoded video access units, such as an encoding pipeline.
pub trait EncodedVideoSource: Send {
    /// Nominal output resolution, used to size the RTC source.
    fn resolution(&self) -> VideoResolution;

    /// Codec produced by this source; fixed for the source's lifetime.
    fn codec(&self) -> EncodedVideoCodec;

    /// Blocks until the next access unit is available, returning `Ok(None)`
    /// when the source reaches the end of its stream.
    fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, SourceError>;

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
    fn update_rate_control(&mut self, _target: RateControl) {}
}

impl<S: PixelVideoSource + ?Sized> PixelVideoSource for Box<S> {
    fn resolution(&self) -> VideoResolution {
        (**self).resolution()
    }

    fn next_frame(&mut self) -> Result<Option<PixelVideoFrame>, SourceError> {
        (**self).next_frame()
    }
}

impl<S: EncodedVideoSource + ?Sized> EncodedVideoSource for Box<S> {
    fn resolution(&self) -> VideoResolution {
        (**self).resolution()
    }

    fn codec(&self) -> EncodedVideoCodec {
        (**self).codec()
    }

    fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, SourceError> {
        (**self).next_access_unit()
    }

    fn request_keyframe(&mut self) {
        (**self).request_keyframe()
    }

    fn update_rate_control(&mut self, target: RateControl) {
        (**self).update_rate_control(target)
    }
}

// Object safety is part of these traits' contract: dynamic applications box
// sources at their edge and drive them through the same generic pumps.
const _: () = {
    fn _assert_object_safe(_: &dyn PixelVideoSource, _: &dyn EncodedVideoSource) {}
};
