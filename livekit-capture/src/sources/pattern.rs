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

//! Test pattern video source.
//!
//! [`PatternVideoSource`] renders a built-in test pattern ([`Pattern`])
//! on the GPU and yields the result as pixel video. Rendering is
//! offscreen through [wgpu], so the source needs no window or display.
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
use livekit::webrtc::video_frame::{BoxVideoFrame, VideoFrame, VideoRotation};
use std::{fmt, time::Duration};
use thiserror::Error;

/// Size of the uniform block: `vec2<f32>` + `f32` + `u32`.
const UNIFORM_BUFFER_SIZE: u64 = 16;

/// Period after which the shader time uniform wraps: 2^13 seconds,
/// about 2.3 hours.
///
/// f32 seconds lose precision as they grow. The wrap keeps the time
/// resolution finer than one millisecond on long runs, at the cost of
/// one pattern discontinuity per period. Frame timestamps do not wrap.
const TIME_WRAP_PERIOD_US: u64 = 8_192_000_000;

/// Prelude prepended to every fragment snippet. It draws one triangle
/// that covers the full target and calls `shade` per pixel.
const FRAGMENT_PRELUDE: &str = include_str!("../../shaders/prelude.wgsl");

/// Fragment snippet for [`Pattern::Gradient`].
const GRADIENT_SHADER: &str = include_str!("../../shaders/gradient.wgsl");

/// Fragment snippet for [`Pattern::Logo`].
const LOGO_SHADER: &str = include_str!("../../shaders/logo.wgsl");

/// Test pattern rendered by a [`PatternVideoSource`].
///
/// Every pattern is a pure function of position, resolution, and time:
/// the same configuration produces the same frames on every machine.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub enum Pattern {
    /// Animated color gradient.
    Gradient,
    /// Bouncing LiveKit logo.
    Logo,
}

impl Pattern {
    /// Returns the complete WGSL module to compile.
    fn module_code(&self) -> String {
        match self {
            Self::Gradient => assemble_module(GRADIENT_SHADER),
            Self::Logo => assemble_module(LOGO_SHADER),
        }
    }
}

/// Prepends the prelude to a pattern's fragment snippet.
fn assemble_module(snippet: &str) -> String {
    format!("{FRAGMENT_PRELUDE}\n{snippet}")
}

/// Configuration for a [`PatternVideoSource`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PatternVideoSourceConfig {
    /// Output resolution.
    pub resolution: VideoResolution,
    /// Output frame rate in frames per second.
    pub framerate_fps: u32,
    /// Pattern to render.
    pub pattern: Pattern,
}

/// Error returned by a pattern video source.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PatternVideoSourceError {
    /// The configured resolution has a zero component.
    #[error("pattern source resolution must be non-zero")]
    ZeroResolution,
    /// The configured frame rate is zero.
    #[error("pattern source frame rate must be non-zero")]
    ZeroFramerate,
    /// The GPU renderer failed.
    #[error(transparent)]
    Render(#[from] RendererError),
}

/// Pixel video source that renders a test pattern on the GPU.
///
/// The source sleeps to pace itself to the configured frame rate. It
/// never reaches the end of its stream — stop the pump that drives it
/// instead.
pub struct PatternVideoSource {
    config: PatternVideoSourceConfig,
    renderer: ShaderRenderer,
    pacer: FramePacer,
}

impl PatternVideoSource {
    /// Creates the source. GPU setup runs on the tokio blocking pool.
    ///
    /// Requires a running tokio runtime. Use
    /// [`PatternVideoSource::new_blocking`] outside of async contexts.
    #[cfg(feature = "tokio")]
    pub async fn new(config: PatternVideoSourceConfig) -> Result<Self, SourceError> {
        crate::utils::run_blocking(move || Self::new_blocking(config)).await
    }

    /// Selects a GPU adapter, compiles the pattern's shader, and builds
    /// the render pipeline.
    ///
    /// Construction fails when no GPU is available, or for a zero
    /// resolution or frame rate.
    pub fn new_blocking(config: PatternVideoSourceConfig) -> Result<Self, SourceError> {
        validate_config(&config).map_err(SourceError::new)?;
        let renderer = ShaderRenderer::new(
            config.resolution,
            &config.pattern.module_code(),
            UNIFORM_BUFFER_SIZE,
        )
        .map_err(|error| SourceError::new(PatternVideoSourceError::Render(error)))?;
        let pacer = FramePacer::new(config.framerate_fps);
        Ok(Self { config, renderer, pacer })
    }

    /// Returns the configuration the source was created with.
    pub fn config(&self) -> &PatternVideoSourceConfig {
        &self.config
    }
}

impl PixelVideoSource for PatternVideoSource {
    fn resolution(&self) -> VideoResolution {
        self.config.resolution
    }

    // The pacing sleep is at most one frame interval, and the renderer
    // bounds every readback wait, so the stop token is observed promptly.
    fn next_frame(&mut self, stop: &PumpStop) -> Result<Option<BoxVideoFrame>, SourceError> {
        let (elapsed, frame_index) = self.pacer.wait_for_next_frame();

        // The shader time wraps to keep its f32 precision on long runs,
        // and the uniform frame index wraps after u32::MAX frames.
        let elapsed_us = elapsed.as_micros() as u64;
        let time_s = Duration::from_micros(elapsed_us % TIME_WRAP_PERIOD_US).as_secs_f32();
        let uniform = uniform_bytes(self.config.resolution, time_s, frame_index as u32);

        let buffer = self
            .renderer
            .render_frame(&uniform, stop)
            .map_err(|error| SourceError::new(PatternVideoSourceError::Render(error)))?;
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

impl fmt::Debug for PatternVideoSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PatternVideoSource").field("config", &self.config).finish_non_exhaustive()
    }
}

/// Validates the CPU-checkable parts of a configuration.
fn validate_config(config: &PatternVideoSourceConfig) -> Result<(), PatternVideoSourceError> {
    let VideoResolution { width, height } = config.resolution;
    if width == 0 || height == 0 {
        return Err(PatternVideoSourceError::ZeroResolution);
    }
    if config.framerate_fps == 0 {
        return Err(PatternVideoSourceError::ZeroFramerate);
    }
    Ok(())
}

/// Serializes the uniform block: resolution, time, and frame index.
fn uniform_bytes(
    resolution: VideoResolution,
    time_s: f32,
    frame_index: u32,
) -> [u8; UNIFORM_BUFFER_SIZE as usize] {
    let mut bytes = [0u8; UNIFORM_BUFFER_SIZE as usize];
    bytes[0..4].copy_from_slice(&(resolution.width as f32).to_ne_bytes());
    bytes[4..8].copy_from_slice(&(resolution.height as f32).to_ne_bytes());
    bytes[8..12].copy_from_slice(&time_s.to_ne_bytes());
    bytes[12..16].copy_from_slice(&frame_index.to_ne_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::gpu_available;

    const RESOLUTION: VideoResolution = VideoResolution { width: 64, height: 36 };

    fn gradient_config() -> PatternVideoSourceConfig {
        PatternVideoSourceConfig {
            resolution: RESOLUTION,
            framerate_fps: 1000,
            pattern: Pattern::Gradient,
        }
    }

    #[test]
    fn validation_rejects_zero_resolution_and_framerate() {
        let mut config = gradient_config();
        config.resolution = VideoResolution::new(0, 36);
        assert!(matches!(validate_config(&config), Err(PatternVideoSourceError::ZeroResolution)));

        let mut config = gradient_config();
        config.framerate_fps = 0;
        assert!(matches!(validate_config(&config), Err(PatternVideoSourceError::ZeroFramerate)));
    }

    #[test]
    fn pattern_modules_include_the_prelude() {
        for pattern in [Pattern::Gradient, Pattern::Logo] {
            let code = pattern.module_code();
            assert!(code.contains("fn vs_main"), "{pattern:?} is missing the prelude");
            assert!(code.contains("fn fs_main"), "{pattern:?} is missing the prelude");
            assert!(code.contains("fn shade"), "{pattern:?} is missing a shade function");
        }
    }

    #[test]
    fn gradient_renders_frames_at_the_frame_rate() {
        if !gpu_available() {
            eprintln!("skipping: no GPU adapter available");
            return;
        }
        let mut source = PatternVideoSource::new_blocking(gradient_config()).unwrap();

        let stop = PumpStop::new();
        let first = source.next_frame(&stop).unwrap().unwrap();
        let second = source.next_frame(&stop).unwrap().unwrap();
        assert_eq!((first.buffer.width(), first.buffer.height()), (64, 36));
        assert_eq!(first.timestamp_us, 0);
        assert_eq!(second.timestamp_us, 1_000);

        // The gradient's top-left pixel at time zero is red-dominant:
        // RGB (255, 68, 47), which is about (121, 91, 211) in
        // limited-range BT.601. A red/blue channel swap in the readback
        // path flips the two chroma values, so this check catches it.
        let i420 = first.buffer.as_i420().expect("pattern source yields I420 buffers");
        let (y, u, v) = i420.data();
        assert!(y[0].abs_diff(121) <= 5, "unexpected luma {}", y[0]);
        assert!(u[0].abs_diff(91) <= 6, "unexpected chroma-u {}", u[0]);
        assert!(v[0].abs_diff(211) <= 6, "unexpected chroma-v {}", v[0]);
    }

    #[test]
    fn logo_renders_on_a_black_background() {
        if !gpu_available() {
            eprintln!("skipping: no GPU adapter available");
            return;
        }
        let mut source = PatternVideoSource::new_blocking(PatternVideoSourceConfig {
            resolution: RESOLUTION,
            framerate_fps: 1000,
            pattern: Pattern::Logo,
        })
        .unwrap();

        let frame = source.next_frame(&PumpStop::new()).unwrap().unwrap();
        let i420 = frame.buffer.as_i420().expect("pattern source yields I420 buffers");
        let (y, _, _) = i420.data();

        // The logo starts away from the corners, so the top-left pixel is
        // background black (luma 16 in limited range), and the lit tile
        // pixels stand out well above it.
        assert!(y[0] <= 20, "top-left pixel is not background: {}", y[0]);
        let lit = y.iter().filter(|&&luma| luma > 60).count();
        assert!(lit > 5, "no logo pixels found (lit count {lit})");
    }
}
