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

use std::fmt;

use thiserror::Error;

use crate::primitives::VideoResolution;

/// Capture backend used by a source implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CaptureBackend {
    /// Let `livekit-capture` choose the platform default backend.
    Auto,
    /// macOS AVFoundation camera capture.
    AvFoundation,
    /// Linux Video4Linux2 camera capture.
    V4l2,
    /// NVIDIA Jetson libargus camera capture.
    LibArgus,
    /// RTSP encoded ingress.
    Rtsp,
    /// TCP byte-stream encoded ingress.
    Tcp,
    /// GStreamer appsink encoded ingress.
    Gstreamer,
}

impl CaptureBackend {
    /// Returns a stable backend name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::AvFoundation => "avfoundation",
            Self::V4l2 => "v4l2",
            Self::LibArgus => "libargus",
            Self::Rtsp => "rtsp",
            Self::Tcp => "tcp",
            Self::Gstreamer => "gstreamer",
        }
    }
}

impl fmt::Display for CaptureBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Capture path used by a source implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CapturePath {
    /// Platform-native uncompressed frame buffers.
    Native,
    /// Uncompressed CPU-accessible frame buffers.
    Raw,
    /// Linux DMA-BUF backed frames.
    DmaBuf,
    /// Compressed encoded access units.
    Encoded,
}

/// Error returned while querying capture devices.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CaptureDeviceQueryError {
    /// The backend does not support device enumeration on this target or build.
    #[error("capture backend {0} does not support device enumeration")]
    UnsupportedBackend(CaptureBackend),
    /// The backend failed while querying devices.
    #[error("capture backend {backend} device query failed: {message}")]
    Backend {
        /// Backend that failed.
        backend: CaptureBackend,
        /// Backend error message.
        message: String,
    },
}

/// Capture device discovered by a platform backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureDeviceInfo {
    /// Backend that reported this device.
    pub backend: CaptureBackend,
    /// Backend-stable device identifier.
    pub id: String,
    /// Preferred selector that reopens this exact device.
    pub selector: CaptureDeviceSelector,
    /// Human-readable device name.
    pub name: String,
    /// Device model identifier, when available.
    pub model_id: Option<String>,
    /// Device manufacturer, when available.
    pub manufacturer: Option<String>,
    /// Capture paths supported by this device.
    pub paths: Vec<CapturePath>,
    /// Capture formats reported by the backend.
    pub formats: Vec<CaptureFormat>,
    /// Whether [`CaptureDeviceInfo::formats`] is a complete backend-reported list.
    pub formats_complete: bool,
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

/// Frame format used by a raw-frame capture backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CaptureFrameFormat {
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
    Grey,
    /// Encoded MJPEG frames.
    Mjpeg,
}

impl CaptureFrameFormat {
    /// Returns a stable lower-case frame-format name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::I420 => "i420",
            Self::Nv12 => "nv12",
            Self::Bgra => "bgra",
            Self::Rgb24 => "rgb24",
            Self::Bgr24 => "bgr24",
            Self::Yuyv => "yuyv",
            Self::Uyvy => "uyvy",
            Self::Grey => "grey",
            Self::Mjpeg => "mjpeg",
        }
    }
}

impl fmt::Display for CaptureFrameFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for CaptureFrameFormat {
    type Err = CaptureFrameFormatParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "i420" | "yuv420p" => Ok(Self::I420),
            "nv12" => Ok(Self::Nv12),
            "bgra" => Ok(Self::Bgra),
            "rgb24" | "rgb" => Ok(Self::Rgb24),
            "bgr24" | "bgr" => Ok(Self::Bgr24),
            "yuyv" | "yuy2" => Ok(Self::Yuyv),
            "uyvy" => Ok(Self::Uyvy),
            "grey" | "greyscale" => Ok(Self::Grey),
            "mjpeg" | "mjpg" => Ok(Self::Mjpeg),
            _ => Err(CaptureFrameFormatParseError),
        }
    }
}

/// Error returned when parsing a [`CaptureFrameFormat`] from a string.
#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
#[error("unknown capture frame format")]
pub struct CaptureFrameFormatParseError;

/// Raw-frame capture format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureFormat {
    /// Frame dimensions.
    pub resolution: VideoResolution,
    /// Frame rate in frames per second.
    pub frame_rate: u32,
    /// Frame format.
    pub frame_format: CaptureFrameFormat,
}

impl CaptureFormat {
    /// Creates a raw-frame capture format.
    pub const fn new(
        resolution: VideoResolution,
        frame_rate: u32,
        frame_format: CaptureFrameFormat,
    ) -> Self {
        Self { resolution, frame_rate, frame_format }
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
    /// Prefer the highest frame rate, optionally constrained by resolution and frame format.
    HighestFrameRate {
        /// Optional resolution constraint.
        resolution: Option<VideoResolution>,
        /// Optional frame format constraint.
        frame_format: Option<CaptureFrameFormat>,
    },
    /// Prefer the highest resolution, optionally constrained by frame rate and frame format.
    HighestResolution {
        /// Optional frame-rate constraint.
        frame_rate: Option<u32>,
        /// Optional frame format constraint.
        frame_format: Option<CaptureFrameFormat>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn capture_frame_format_parses_common_names() {
        assert_eq!(CaptureFrameFormat::from_str("MJPEG"), Ok(CaptureFrameFormat::Mjpeg));
        assert_eq!(CaptureFrameFormat::from_str("mjpg"), Ok(CaptureFrameFormat::Mjpeg));
        assert_eq!(CaptureFrameFormat::from_str("grey"), Ok(CaptureFrameFormat::Grey));
        assert_eq!(CaptureFrameFormat::from_str("GREY"), Ok(CaptureFrameFormat::Grey));
        assert_eq!(CaptureFrameFormat::from_str("yuy2"), Ok(CaptureFrameFormat::Yuyv));
    }

    #[test]
    fn capture_frame_format_displays_canonical_names() {
        assert_eq!(CaptureFrameFormat::Mjpeg.to_string(), "mjpeg");
        assert_eq!(CaptureFrameFormat::Grey.to_string(), "grey");
    }

    #[test]
    fn device_info_can_report_incomplete_format_lists() {
        let info = CaptureDeviceInfo {
            backend: CaptureBackend::AvFoundation,
            id: "camera-0".to_string(),
            selector: CaptureDeviceSelector::Id("camera-0".to_string()),
            name: "Camera".to_string(),
            model_id: None,
            manufacturer: None,
            paths: vec![CapturePath::Native, CapturePath::Raw],
            formats: Vec::new(),
            formats_complete: false,
        };

        assert_eq!(info.backend, CaptureBackend::AvFoundation);
        assert_eq!(info.selector, CaptureDeviceSelector::Id("camera-0".to_string()));
        assert_eq!(info.paths, vec![CapturePath::Native, CapturePath::Raw]);
        assert!(!info.formats_complete);
    }
}
