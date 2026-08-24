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

//! Camera device capture.
//!
//! [`DeviceVideoSource`] captures pixel frames from a video device through
//! the platform's native capture stack. Configuration, enumeration
//! ([`devices`]), and errors use one platform-neutral vocabulary. On
//! platforms without a backend the module still compiles, and construction
//! and enumeration fail with
//! [`DeviceVideoSourceError::UnsupportedPlatform`].
//!
//! Where the platform supports it, frames reach the RTC track as
//! platform-native buffers without a CPU copy. Otherwise they are converted
//! to I420.

#[cfg(target_os = "macos")]
mod avfoundation;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod timestamp;
#[cfg(target_os = "linux")]
mod v4l2;

#[cfg(target_os = "macos")]
use avfoundation as backend;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use unsupported as backend;
#[cfg(target_os = "linux")]
use v4l2 as backend;

use std::fmt;

use livekit::webrtc::video_frame::BoxVideoFrame;
use thiserror::Error;

use crate::{
    error::SourceError, pixel::PixelVideoSource, primitive::VideoResolution, pump::PumpStop,
};

/// Selects the video device a [`DeviceVideoSource`] captures from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub enum DeviceSelector {
    /// The platform default video device.
    #[default]
    Default,
    /// The device at this position in the platform enumeration order.
    Index(usize),
    /// The device with this identifier, as reported by [`DeviceInfo::id`].
    ///
    /// Identifiers are backend-specific and treated as opaque; prefer
    /// [`DeviceInfo::selector`] over constructing them. On Linux, Jetson CSI
    /// sensors captured through libargus use the `argus:N` namespace.
    Id(String),
}

/// Frame format delivered by a capture device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub enum DeviceFrameFormat {
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

impl DeviceFrameFormat {
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

impl fmt::Display for DeviceFrameFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for DeviceFrameFormat {
    type Err = DeviceFrameFormatParseError;

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
            _ => Err(DeviceFrameFormatParseError),
        }
    }
}

/// Error returned when parsing a [`DeviceFrameFormat`] from a string.
#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
#[error("unknown device frame format")]
pub struct DeviceFrameFormatParseError;

/// Capture format offered by a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct DeviceFormat {
    /// Frame dimensions.
    pub resolution: VideoResolution,
    /// Frame rate in frames per second.
    pub framerate_fps: u32,
    /// Frame format.
    pub frame_format: DeviceFrameFormat,
}

impl DeviceFormat {
    /// Creates a device capture format.
    pub const fn new(
        resolution: VideoResolution,
        framerate_fps: u32,
        frame_format: DeviceFrameFormat,
    ) -> Self {
        Self { resolution, framerate_fps, frame_format }
    }
}

impl fmt::Display for DeviceFormat {
    /// Formats as `WIDTHxHEIGHT@FPSfps FORMAT`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}fps {}", self.resolution, self.framerate_fps, self.frame_format)
    }
}

/// Format selection requested from a capture device.
///
/// The device negotiates the delivered format, and
/// [`DeviceVideoSource::format`] reports the outcome.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub enum DeviceFormatRequest {
    /// Let the device choose its default format.
    #[default]
    Default,
    /// Require an exact format match.
    Exact(DeviceFormat),
    /// Use the device's closest supported format.
    Closest(DeviceFormat),
    /// Prefer the highest frame rate, optionally constrained by resolution
    /// and frame format.
    HighestFramerate {
        /// Optional resolution constraint.
        #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
        resolution: Option<VideoResolution>,
        /// Optional frame format constraint.
        #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
        frame_format: Option<DeviceFrameFormat>,
    },
    /// Prefer the highest resolution, optionally constrained by frame rate
    /// and frame format.
    HighestResolution {
        /// Optional frame-rate constraint.
        #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
        framerate_fps: Option<u32>,
        /// Optional frame format constraint.
        #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
        frame_format: Option<DeviceFrameFormat>,
    },
}

/// Video capture device discovered by [`devices`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct DeviceInfo {
    /// Device identifier, usable with [`DeviceSelector::Id`].
    pub id: String,
    /// Human-readable device name.
    pub name: String,
    /// Device model identifier, when available.
    pub model_id: Option<String>,
    /// Device manufacturer, when available.
    pub manufacturer: Option<String>,
    /// Capture formats reported by the device.
    pub formats: Vec<DeviceFormat>,
    /// Whether [`DeviceInfo::formats`] is a complete list. Some platforms do
    /// not enumerate formats up front.
    pub formats_complete: bool,
}

impl DeviceInfo {
    /// Returns the selector that reopens this exact device.
    pub fn selector(&self) -> DeviceSelector {
        DeviceSelector::Id(self.id.clone())
    }
}

/// Lists the video capture devices on this machine.
///
/// Requires a running tokio runtime: enumeration runs on the tokio blocking
/// pool. Use [`devices_blocking`] outside of async contexts.
#[cfg(feature = "tokio")]
pub async fn devices() -> Result<Vec<DeviceInfo>, SourceError> {
    crate::utils::run_blocking(devices_blocking).await
}

/// Lists the video capture devices on this machine.
///
/// Enumeration queries the platform capture stack and can block briefly.
pub fn devices_blocking() -> Result<Vec<DeviceInfo>, SourceError> {
    backend::devices().map_err(SourceError::new)
}

/// Configuration for a [`DeviceVideoSource`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct DeviceVideoSourceConfig {
    /// Device to capture from.
    #[cfg_attr(feature = "serde", serde(default))]
    pub device: DeviceSelector,
    /// Format requested from the device.
    #[cfg_attr(feature = "serde", serde(default))]
    pub format: DeviceFormatRequest,
}

/// Pixel video source that captures frames from a video device, such as a
/// camera.
///
/// Construction opens the device and negotiates the capture format, so
/// [`DeviceVideoSource::format`] is known before any frame is pumped. The
/// source never reaches the end of its stream — stop the pump that drives
/// it instead.
///
/// Frames carry a monotonic `timestamp_us`. Each frame's `frame_metadata`
/// is pre-filled with the wall-clock capture time — the device's own
/// capture timestamp when the platform reports a valid one.
pub struct DeviceVideoSource {
    config: DeviceVideoSourceConfig,
    format: DeviceFormat,
    session: backend::Session,
}

impl DeviceVideoSource {
    /// Creates the source. Device negotiation runs on the tokio blocking
    /// pool.
    ///
    /// Requires a running tokio runtime. Use
    /// [`DeviceVideoSource::new_blocking`] outside of async contexts.
    #[cfg(feature = "tokio")]
    pub async fn new(config: DeviceVideoSourceConfig) -> Result<Self, SourceError> {
        crate::utils::run_blocking(move || Self::new_blocking(config)).await
    }

    /// Opens the configured device and negotiates the capture format.
    ///
    /// This can block until the device delivers its first frame, bounded by
    /// a timeout. Construction fails on a missing device, a format request
    /// the device cannot satisfy, or a platform without a capture backend.
    pub fn new_blocking(config: DeviceVideoSourceConfig) -> Result<Self, SourceError> {
        let session = backend::Session::open(&config).map_err(SourceError::new)?;
        let format = session.format();
        Ok(Self { config, format, session })
    }

    /// Returns the configuration the source was created with.
    pub fn config(&self) -> &DeviceVideoSourceConfig {
        &self.config
    }

    /// Returns the negotiated capture format.
    ///
    /// The resolution matches what [`PixelVideoSource::resolution`] reports.
    /// The frame format is what the device delivers before any conversion.
    pub fn format(&self) -> DeviceFormat {
        self.format
    }
}

impl PixelVideoSource for DeviceVideoSource {
    fn resolution(&self) -> VideoResolution {
        self.format.resolution
    }

    // Backends bound every blocking wait so the stop token is observed
    // within ~100ms even when the device stalls.
    fn next_frame(&mut self, stop: &PumpStop) -> Result<Option<BoxVideoFrame>, SourceError> {
        self.session.next_frame(stop).map_err(SourceError::new)
    }
}

impl fmt::Debug for DeviceVideoSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceVideoSource")
            .field("config", &self.config)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

/// Error returned by device capture.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DeviceVideoSourceError {
    /// Device capture has no backend for this platform.
    #[error("device capture is not supported on this platform")]
    UnsupportedPlatform,
    /// The requested device was not found.
    #[error("capture device was not found")]
    DeviceNotFound,
    /// The configuration is invalid.
    #[error("invalid device source configuration: {0}")]
    InvalidConfig(&'static str),
    /// The requested frame format is not supported by this platform's
    /// backend.
    #[error("device capture does not support frame format {0} on this platform")]
    UnsupportedFrameFormat(DeviceFrameFormat),
    /// The requested capture format is not available on the selected device.
    #[error("capture format is not available on the device: {0}")]
    UnsupportedFormat(DeviceFormat),
    /// Timed out waiting for the device to deliver a frame.
    #[error("timed out waiting for a frame from the capture device")]
    FrameTimeout,
    /// Captured frame bytes did not match the negotiated format.
    #[error("invalid captured frame: {0}")]
    InvalidFrame(&'static str),
    /// Pixel conversion failed.
    #[error("failed to convert captured frame to I420: {0}")]
    Convert(&'static str),
    /// Compressed frame decoding failed.
    #[error("failed to decode compressed frame: {0}")]
    Decode(String),
    /// The platform capture stack reported an error.
    #[error("capture device error: {0}")]
    Backend(String),
}

/// Builds the packet-trailer metadata that device frames are pre-filled
/// with. A metadata callback set on the pump takes precedence.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn capture_frame_metadata(
    capture_wall_time_us: u64,
) -> livekit::webrtc::video_frame::FrameMetadata {
    livekit::webrtc::video_frame::FrameMetadata {
        user_timestamp: Some(capture_wall_time_us),
        frame_id: None,
        user_data: None,
    }
}

/// Validates the platform-neutral parts of a configuration; `supported`
/// reports whether the backend can deliver a frame format.
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
fn validate_config(
    config: &DeviceVideoSourceConfig,
    supported: fn(DeviceFrameFormat) -> bool,
) -> Result<(), DeviceVideoSourceError> {
    if let DeviceSelector::Id(id) = &config.device {
        if id.is_empty() {
            return Err(DeviceVideoSourceError::InvalidConfig("device id must be non-empty"));
        }
    }

    let validate_frame_format = |frame_format: DeviceFrameFormat| {
        if !supported(frame_format) {
            return Err(DeviceVideoSourceError::UnsupportedFrameFormat(frame_format));
        }
        Ok(())
    };
    let validate_resolution = |resolution: VideoResolution| {
        if resolution.width == 0 {
            return Err(DeviceVideoSourceError::InvalidConfig("width must be non-zero"));
        }
        if resolution.height == 0 {
            return Err(DeviceVideoSourceError::InvalidConfig("height must be non-zero"));
        }
        Ok(())
    };

    match &config.format {
        DeviceFormatRequest::Default => Ok(()),
        DeviceFormatRequest::Exact(format) | DeviceFormatRequest::Closest(format) => {
            validate_resolution(format.resolution)?;
            if format.framerate_fps == 0 {
                return Err(DeviceVideoSourceError::InvalidConfig(
                    "framerate_fps must be non-zero",
                ));
            }
            validate_frame_format(format.frame_format)
        }
        DeviceFormatRequest::HighestFramerate { resolution, frame_format } => {
            if let Some(resolution) = resolution {
                validate_resolution(*resolution)?;
            }
            if let Some(frame_format) = frame_format {
                validate_frame_format(*frame_format)?;
            }
            Ok(())
        }
        DeviceFormatRequest::HighestResolution { framerate_fps, frame_format } => {
            if matches!(framerate_fps, Some(0)) {
                return Err(DeviceVideoSourceError::InvalidConfig(
                    "framerate_fps must be non-zero",
                ));
            }
            if let Some(frame_format) = frame_format {
                validate_frame_format(*frame_format)?;
            }
            Ok(())
        }
    }
}

/// Stub backend for platforms without device capture.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod unsupported {
    use livekit::webrtc::video_frame::BoxVideoFrame;

    use super::{DeviceFormat, DeviceInfo, DeviceVideoSourceConfig, DeviceVideoSourceError};
    use crate::pump::PumpStop;

    /// Uninhabited: [`Session::open`] always fails on this platform.
    #[derive(Debug)]
    pub(super) enum Session {}

    impl Session {
        pub(super) fn open(
            _config: &DeviceVideoSourceConfig,
        ) -> Result<Self, DeviceVideoSourceError> {
            Err(DeviceVideoSourceError::UnsupportedPlatform)
        }

        pub(super) fn format(&self) -> DeviceFormat {
            match *self {}
        }

        pub(super) fn next_frame(
            &mut self,
            _stop: &PumpStop,
        ) -> Result<Option<BoxVideoFrame>, DeviceVideoSourceError> {
            match *self {}
        }
    }

    pub(super) fn devices() -> Result<Vec<DeviceInfo>, DeviceVideoSourceError> {
        Err(DeviceVideoSourceError::UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn any_supported(_: DeviceFrameFormat) -> bool {
        true
    }

    #[test]
    fn frame_format_parses_common_names() {
        assert_eq!(DeviceFrameFormat::from_str("MJPEG"), Ok(DeviceFrameFormat::Mjpeg));
        assert_eq!(DeviceFrameFormat::from_str("mjpg"), Ok(DeviceFrameFormat::Mjpeg));
        assert_eq!(DeviceFrameFormat::from_str("grey"), Ok(DeviceFrameFormat::Grey));
        assert_eq!(DeviceFrameFormat::from_str("GREY"), Ok(DeviceFrameFormat::Grey));
        assert_eq!(DeviceFrameFormat::from_str("yuy2"), Ok(DeviceFrameFormat::Yuyv));
    }

    #[test]
    fn frame_format_displays_canonical_names() {
        assert_eq!(DeviceFrameFormat::Mjpeg.to_string(), "mjpeg");
        assert_eq!(DeviceFrameFormat::Grey.to_string(), "grey");
    }

    #[test]
    fn validation_rejects_empty_device_id() {
        let config = DeviceVideoSourceConfig {
            device: DeviceSelector::Id(String::new()),
            format: DeviceFormatRequest::Default,
        };
        assert!(matches!(
            validate_config(&config, any_supported),
            Err(DeviceVideoSourceError::InvalidConfig(_))
        ));
    }

    #[test]
    fn validation_rejects_zero_format_components() {
        let zero_width = DeviceVideoSourceConfig {
            device: DeviceSelector::Default,
            format: DeviceFormatRequest::Exact(DeviceFormat::new(
                VideoResolution::new(0, 720),
                30,
                DeviceFrameFormat::Yuyv,
            )),
        };
        assert!(matches!(
            validate_config(&zero_width, any_supported),
            Err(DeviceVideoSourceError::InvalidConfig(_))
        ));

        let zero_framerate = DeviceVideoSourceConfig {
            device: DeviceSelector::Default,
            format: DeviceFormatRequest::HighestResolution {
                framerate_fps: Some(0),
                frame_format: None,
            },
        };
        assert!(matches!(
            validate_config(&zero_framerate, any_supported),
            Err(DeviceVideoSourceError::InvalidConfig(_))
        ));
    }

    #[test]
    fn validation_rejects_unsupported_frame_formats() {
        let config = DeviceVideoSourceConfig {
            device: DeviceSelector::Default,
            format: DeviceFormatRequest::HighestFramerate {
                resolution: None,
                frame_format: Some(DeviceFrameFormat::Uyvy),
            },
        };
        assert!(matches!(
            validate_config(&config, |format| format != DeviceFrameFormat::Uyvy),
            Err(DeviceVideoSourceError::UnsupportedFrameFormat(DeviceFrameFormat::Uyvy))
        ));
    }

    #[test]
    fn default_config_requests_default_device_and_format() {
        let config = DeviceVideoSourceConfig::default();
        assert_eq!(config.device, DeviceSelector::Default);
        assert_eq!(config.format, DeviceFormatRequest::Default);
    }

    #[test]
    fn device_info_selector_reopens_by_id() {
        let info = DeviceInfo {
            id: "camera-0".to_string(),
            name: "Camera".to_string(),
            model_id: None,
            manufacturer: None,
            formats: Vec::new(),
            formats_complete: false,
        };
        assert_eq!(info.selector(), DeviceSelector::Id("camera-0".to_string()));
    }
}
