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

//! Pixel (unencoded) video: the source trait and its pump.
//!
//! A source yields libwebrtc [`VideoFrame`](livekit::webrtc::video_frame::VideoFrame)s,
//! so any [`VideoBuffer`](livekit::webrtc::video_frame::VideoBuffer) — CPU
//! planes or platform-native — passes to the RTC source without an
//! intermediate copy. [`PixelVideoPump`] drives a source and publishes its
//! frames through the WebRTC encoder.
//!
//! [`PixelVideoSource`] is object-safe and implemented for `Box<dyn ...>`,
//! so sources constructed dynamically run through the same generic pump.

mod pump;

use livekit::webrtc::video_frame::BoxVideoFrame;

pub use pump::PixelVideoPump;

use crate::{error::SourceError, primitive::VideoResolution, pump::PumpStop};

/// Source of pixel (unencoded) video frames, such as a camera device.
pub trait PixelVideoSource: Send {
    /// Nominal output resolution, used to size the RTC source.
    fn resolution(&self) -> VideoResolution;

    /// Blocks until the next frame is available. Returns `Ok(None)` at the
    /// end of the stream.
    ///
    /// Implementations must return `Ok(None)` promptly once `stop` fires:
    /// integrate the token into the blocking wait, or bound each wait to
    /// about one frame interval. The pump uses the token to tell a stop
    /// from the end of the stream.
    ///
    /// Implementations can pre-fill the frame's `frame_metadata`. A
    /// metadata callback set on the pump takes precedence when it returns
    /// `Some`.
    fn next_frame(&mut self, stop: &PumpStop) -> Result<Option<BoxVideoFrame>, SourceError>;
}

impl<S: PixelVideoSource + ?Sized> PixelVideoSource for Box<S> {
    fn resolution(&self) -> VideoResolution {
        (**self).resolution()
    }

    fn next_frame(&mut self, stop: &PumpStop) -> Result<Option<BoxVideoFrame>, SourceError> {
        (**self).next_frame(stop)
    }
}

// Object safety is part of this trait's contract: dynamic applications box
// sources at their edge and drive them through the same generic pumps.
const _: () = {
    fn _assert_object_safe(_: &dyn PixelVideoSource) {}
};
