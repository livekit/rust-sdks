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

//! Domain-neutral video primitives shared across capture paths and backends.
//!
//! These types carry no capture- or codec-specific semantics, so they can serve
//! as a common vocabulary for frame geometry and related quantities across
//! crates.

// TODO: in a future refactor, move these types into their own
// crate (e.g., `livekit-video-primitives`) so all crates in this workspace can work
// with common types without creating undesirable dependencies.

/// Pixel dimensions of a video frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct VideoResolution {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
}

impl VideoResolution {
    /// Creates a video resolution from a width and height in pixels.
    ///
    /// ```
    /// # use livekit_capture::primitives::VideoResolution;
    /// let resolution = VideoResolution::new(1920, 1080);
    /// assert_eq!(resolution.width, 1920);
    /// assert_eq!(resolution.height, 1080);
    /// ```
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Returns the ratio between the width and heigh components.
    ///
    /// If heigh component is zero, the result is `None`.
    ///
    /// ```
    /// # use livekit_capture::primitives::VideoResolution;
    /// assert_eq!(VideoResolution::new(1920, 960).aspect_ratio(), Some(2.0));
    /// assert_eq!(VideoResolution::new(1920, 0).aspect_ratio(), None);
    /// ```
    ///
    pub fn aspect_ratio(&self) -> Option<f64> {
        if self.height == 0 {
            return None;
        }
        Some(f64::from(self.width) / f64::from(self.height))
    }
}

impl From<VideoResolution> for livekit::webrtc::video_source::VideoResolution {
    fn from(value: VideoResolution) -> Self {
        Self { width: value.width, height: value.height }
    }
}
