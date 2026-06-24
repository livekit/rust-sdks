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

use livekit::webrtc::video_source::VideoResolution;

/// Capture device discovered by a platform backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureDeviceInfo {
    /// Backend-stable device identifier.
    pub id: String,
    /// Human-readable device name.
    pub name: String,
    /// Device model identifier, when available.
    pub model_id: Option<String>,
    /// Device manufacturer, when available.
    pub manufacturer: Option<String>,
    /// Capture formats reported by the backend.
    pub formats: Vec<CaptureFormat>,
}

/// Device selector used by capture backends.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CaptureDeviceSelector {
    /// Use the backend default video device.
    Default,
    /// Use the device at the backend enumeration index.
    Index(usize),
    /// Use a backend-stable device identifier.
    Id(String),
}

/// Pixel format used by a decoded-frame capture backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CapturePixelFormat {
    /// Planar I420/YUV420P.
    I420,
    /// Biplanar NV12.
    Nv12,
    /// Packed BGRA.
    Bgra,
    /// Packed RGB24.
    Rgb24,
    /// Packed BGR24.
    Bgr24,
    /// Packed YUYV/YUY2.
    Yuyv,
    /// Packed UYVY.
    Uyvy,
    /// Single-plane 8-bit luma.
    Gray,
    /// Encoded MJPEG frames.
    Mjpeg,
}

/// Pixel dimensions for a capture format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureResolution {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
}

impl CaptureResolution {
    /// Creates a capture resolution.
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

impl From<CaptureResolution> for VideoResolution {
    fn from(value: CaptureResolution) -> Self {
        Self { width: value.width, height: value.height }
    }
}

/// Decoded-frame capture format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureFormat {
    /// Frame dimensions.
    pub resolution: CaptureResolution,
    /// Frame rate in frames per second.
    pub frame_rate: u32,
    /// Pixel format.
    pub pixel_format: CapturePixelFormat,
}

impl CaptureFormat {
    /// Creates a decoded-frame capture format.
    pub const fn new(
        resolution: CaptureResolution,
        frame_rate: u32,
        pixel_format: CapturePixelFormat,
    ) -> Self {
        Self { resolution, frame_rate, pixel_format }
    }
}

/// Format selection requested from a capture backend.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CaptureFormatRequest {
    /// Let the backend choose its default format.
    Default,
    /// Require an exact format match.
    Exact(CaptureFormat),
    /// Use the backend's closest supported format.
    Closest(CaptureFormat),
    /// Prefer the highest frame rate, optionally constrained by resolution and pixel format.
    HighestFrameRate {
        /// Optional resolution constraint.
        resolution: Option<CaptureResolution>,
        /// Optional pixel format constraint.
        pixel_format: Option<CapturePixelFormat>,
    },
    /// Prefer the highest resolution, optionally constrained by frame rate and pixel format.
    HighestResolution {
        /// Optional frame-rate constraint.
        frame_rate: Option<u32>,
        /// Optional pixel format constraint.
        pixel_format: Option<CapturePixelFormat>,
    },
}
