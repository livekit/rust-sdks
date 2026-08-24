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

//! Wall-clock video source, for latency measurement.
//!
//! [`ClockVideoSource`] renders the local time as HH:MM:SS.mmm on the
//! GPU, with a grid below the digits that shows the milliseconds as
//! filled cells. The source samples the wall clock once per frame.
//! Rendering is offscreen through [wgpu], so the source needs no window
//! or display.
//!
//! The source reads each frame back from the GPU and converts it to I420
//! on the CPU.
//!
//! [wgpu]: https://wgpu.rs

use crate::{
    error::SourceError,
    pixel::PixelVideoSource,
    primitive::VideoResolution,
    pump::PumpStop,
    renderer::{FramePacer, RendererError, ShaderRenderer},
};
use chrono::Timelike;
use livekit::webrtc::video_frame::{BoxVideoFrame, VideoFrame, VideoRotation};
use std::fmt;
use thiserror::Error;

/// Complete WGSL module for the clock.
const CLOCK_SHADER: &str = include_str!("../../shaders/clock.wgsl");

/// Number of characters on the clock face: HH:MM:SS.mmm.
const CHAR_COUNT: usize = 12;

/// Character codes for the separators. Codes 0 to 9 are digits.
const COLON: u32 = 10;
const DOT: u32 = 11;

/// Size of the uniform block: `vec2<f32>` + padding + 3 * `vec4<u32>`.
const UNIFORM_BUFFER_SIZE: u64 = 64;

/// Configuration for a [`ClockVideoSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ClockVideoSourceConfig {
    /// Output resolution.
    pub resolution: VideoResolution,
    /// Output frame rate in frames per second.
    pub framerate_fps: u32,
}

/// Error returned by a clock video source.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ClockVideoSourceError {
    /// The configured resolution has a zero component.
    #[error("clock source resolution must be non-zero")]
    ZeroResolution,
    /// The configured frame rate is zero.
    #[error("clock source frame rate must be non-zero")]
    ZeroFramerate,
    /// The GPU renderer failed.
    #[error(transparent)]
    Render(#[from] RendererError),
}

/// Pixel video source that renders a wall clock with millisecond
/// precision.
///
/// The source sleeps to pace itself to the configured frame rate. It
/// never reaches the end of its stream — stop the pump that drives it
/// instead.
pub struct ClockVideoSource {
    config: ClockVideoSourceConfig,
    renderer: ShaderRenderer,
    pacer: FramePacer,
}

impl ClockVideoSource {
    /// Creates the source. GPU setup runs on the tokio blocking pool.
    ///
    /// Requires a running tokio runtime. Use
    /// [`ClockVideoSource::new_blocking`] outside of async contexts.
    #[cfg(feature = "tokio")]
    pub async fn new(config: ClockVideoSourceConfig) -> Result<Self, SourceError> {
        crate::utils::run_blocking(move || Self::new_blocking(config)).await
    }

    /// Selects a GPU adapter, compiles the clock shader, and builds the
    /// render pipeline.
    ///
    /// Construction fails when no GPU is available, or for a zero
    /// resolution or frame rate.
    pub fn new_blocking(config: ClockVideoSourceConfig) -> Result<Self, SourceError> {
        validate_config(&config).map_err(SourceError::new)?;
        let renderer = ShaderRenderer::new(config.resolution, CLOCK_SHADER, UNIFORM_BUFFER_SIZE)
            .map_err(|error| SourceError::new(ClockVideoSourceError::Render(error)))?;
        let pacer = FramePacer::new(config.framerate_fps);
        Ok(Self { config, renderer, pacer })
    }

    /// Returns the configuration the source was created with.
    pub fn config(&self) -> ClockVideoSourceConfig {
        self.config
    }
}

impl PixelVideoSource for ClockVideoSource {
    fn resolution(&self) -> VideoResolution {
        self.config.resolution
    }

    // The pacing sleep is at most one frame interval, and the renderer
    // bounds every readback wait, so the stop token is observed promptly.
    fn next_frame(&mut self, stop: &PumpStop) -> Result<Option<BoxVideoFrame>, SourceError> {
        let (elapsed, _) = self.pacer.wait_for_next_frame();

        // Sample the wall clock after the pacing sleep, so the shown
        // time is as close as possible to the capture time.
        let now = chrono::Local::now();
        let chars =
            clock_chars(now.hour(), now.minute(), now.second(), now.nanosecond() / 1_000_000);
        let uniform = uniform_bytes(self.config.resolution, &chars);

        let buffer = self
            .renderer
            .render_frame(&uniform, stop)
            .map_err(|error| SourceError::new(ClockVideoSourceError::Render(error)))?;
        let Some(buffer) = buffer else {
            // The stop token fired during the readback wait.
            return Ok(None);
        };

        Ok(Some(VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: elapsed.as_micros() as i64,
            frame_metadata: None,
            buffer: Box::new(buffer),
        }))
    }
}

impl fmt::Debug for ClockVideoSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClockVideoSource").field("config", &self.config).finish_non_exhaustive()
    }
}

/// Validates the CPU-checkable parts of a configuration.
fn validate_config(config: &ClockVideoSourceConfig) -> Result<(), ClockVideoSourceError> {
    let VideoResolution { width, height } = config.resolution;
    if width == 0 || height == 0 {
        return Err(ClockVideoSourceError::ZeroResolution);
    }
    if config.framerate_fps == 0 {
        return Err(ClockVideoSourceError::ZeroFramerate);
    }
    Ok(())
}

/// Returns the twelve character codes for HH:MM:SS.mmm.
fn clock_chars(hour: u32, minute: u32, second: u32, millisecond: u32) -> [u32; CHAR_COUNT] {
    [
        (hour / 10) % 10,
        hour % 10,
        COLON,
        (minute / 10) % 10,
        minute % 10,
        COLON,
        (second / 10) % 10,
        second % 10,
        DOT,
        (millisecond / 100) % 10,
        (millisecond / 10) % 10,
        millisecond % 10,
    ]
}

/// Serializes the uniform block: viewport size, padding, and the twelve
/// character codes at their 16-byte-aligned offset.
fn uniform_bytes(
    resolution: VideoResolution,
    chars: &[u32; CHAR_COUNT],
) -> [u8; UNIFORM_BUFFER_SIZE as usize] {
    let mut bytes = [0u8; UNIFORM_BUFFER_SIZE as usize];
    bytes[0..4].copy_from_slice(&(resolution.width as f32).to_ne_bytes());
    bytes[4..8].copy_from_slice(&(resolution.height as f32).to_ne_bytes());
    for (index, code) in chars.iter().enumerate() {
        let offset = 16 + index * 4;
        bytes[offset..offset + 4].copy_from_slice(&code.to_ne_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::gpu_available;

    fn test_config() -> ClockVideoSourceConfig {
        ClockVideoSourceConfig {
            resolution: VideoResolution { width: 320, height: 180 },
            framerate_fps: 1000,
        }
    }

    #[test]
    fn clock_chars_render_three_millisecond_digits() {
        assert_eq!(clock_chars(12, 34, 56, 789), [1, 2, COLON, 3, 4, COLON, 5, 6, DOT, 7, 8, 9]);
    }

    #[test]
    fn validation_rejects_zero_resolution_and_framerate() {
        let mut config = test_config();
        config.resolution = VideoResolution::new(0, 180);
        assert!(matches!(validate_config(&config), Err(ClockVideoSourceError::ZeroResolution)));

        let mut config = test_config();
        config.framerate_fps = 0;
        assert!(matches!(validate_config(&config), Err(ClockVideoSourceError::ZeroFramerate)));
    }

    #[test]
    fn uniform_places_chars_at_their_alignment() {
        let chars = clock_chars(12, 34, 56, 789);
        let bytes = uniform_bytes(VideoResolution::new(1280, 720), &chars);
        assert_eq!(f32::from_ne_bytes(bytes[0..4].try_into().unwrap()), 1280.0);
        assert_eq!(f32::from_ne_bytes(bytes[4..8].try_into().unwrap()), 720.0);
        assert_eq!(u32::from_ne_bytes(bytes[16..20].try_into().unwrap()), 1);
        assert_eq!(u32::from_ne_bytes(bytes[60..64].try_into().unwrap()), 9);
    }

    #[test]
    fn clock_renders_frames() {
        if !gpu_available() {
            eprintln!("skipping: no GPU adapter available");
            return;
        }
        let mut source = ClockVideoSource::new_blocking(test_config()).unwrap();

        let stop = PumpStop::new();
        let first = source.next_frame(&stop).unwrap().unwrap();
        let second = source.next_frame(&stop).unwrap().unwrap();
        assert_eq!((first.buffer.width(), first.buffer.height()), (320, 180));
        assert_eq!(first.timestamp_us, 0);
        assert_eq!(second.timestamp_us, 1_000);

        // The clock is centered with a margin, so the top-left pixel is
        // background black, and the lit digits stand out well above it.
        let i420 = first.buffer.as_i420().expect("clock source yields I420 buffers");
        let (y, _, _) = i420.data();
        assert!(y[0] <= 20, "top-left pixel is not background: {}", y[0]);
        let lit = y.iter().filter(|&&luma| luma > 100).count();
        assert!(lit > 20, "no clock pixels found (lit count {lit})");
    }
}
