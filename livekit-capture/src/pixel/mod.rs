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

//! Pixel (unencoded) video: frame types, the source contract, and the pump.
//!
//! Sources produce crate-owned frame types independent of libwebrtc, and
//! [`PixelVideoPump`] bridges them into an RTC track. The source trait is
//! object-safe and implemented for `Box<dyn ...>`, so sources can be
//! constructed dynamically and driven through the same generic pump.

mod pump;

use bytes::Bytes;

pub use pump::PixelVideoPump;

use crate::{error::SourceError, primitive::VideoResolution};

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
    /// Frame resolution in pixels.
    pub resolution: VideoResolution,
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

impl<S: PixelVideoSource + ?Sized> PixelVideoSource for Box<S> {
    fn resolution(&self) -> VideoResolution {
        (**self).resolution()
    }

    fn next_frame(&mut self) -> Result<Option<PixelVideoFrame>, SourceError> {
        (**self).next_frame()
    }
}

// Object safety is part of this trait's contract: dynamic applications box
// sources at their edge and drive them through the same generic pumps.
const _: () = {
    fn _assert_object_safe(_: &dyn PixelVideoSource) {}
};
