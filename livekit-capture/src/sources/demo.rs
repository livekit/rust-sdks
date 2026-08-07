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

//! Demo video source for testing: an animated color gradient.

use crate::{
    error::SourceError,
    pixel::PixelVideoSource,
    primitive::VideoResolution,
    pump::PumpStop,
    sources::shader::{ShaderVideoSource, ShaderVideoSourceConfig, WgslShader},
};
use livekit::webrtc::video_frame::BoxVideoFrame;

/// Fragment snippet the demo source renders.
const DEMO_SHADER: &str = include_str!("../../shaders/demo.wgsl");

/// Configuration for a [`DemoVideoSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct DemoVideoSourceConfig {
    /// Output resolution.
    pub resolution: VideoResolution,
    /// Output frame rate in frames per second.
    pub framerate_fps: u32,
}

/// Pixel video source that renders an animated color gradient.
///
/// This is a convenience wrapper around [`ShaderVideoSource`] with a
/// built-in shader. The source paces itself to the configured frame
/// rate. It never reaches the end of its stream — stop the pump that
/// drives it instead.
#[derive(Debug)]
pub struct DemoVideoSource {
    config: DemoVideoSourceConfig,
    inner: ShaderVideoSource,
}

impl DemoVideoSource {
    /// Creates the source. GPU setup runs on the tokio blocking pool.
    ///
    /// Requires a running tokio runtime. Use
    /// [`DemoVideoSource::new_blocking`] outside of async contexts.
    #[cfg(feature = "tokio")]
    pub async fn new(config: DemoVideoSourceConfig) -> Result<Self, SourceError> {
        crate::utils::run_blocking(move || Self::new_blocking(config)).await
    }

    /// Creates the source and its GPU state.
    ///
    /// Construction fails when no GPU is available, or for a zero
    /// resolution or frame rate.
    pub fn new_blocking(config: DemoVideoSourceConfig) -> Result<Self, SourceError> {
        let inner = ShaderVideoSource::new_blocking(ShaderVideoSourceConfig {
            resolution: config.resolution,
            framerate_fps: config.framerate_fps,
            shader: WgslShader::Fragment(DEMO_SHADER.to_owned()),
        })?;
        Ok(Self { config, inner })
    }

    /// Returns the configuration the source was created with.
    pub fn config(&self) -> DemoVideoSourceConfig {
        self.config
    }
}

impl PixelVideoSource for DemoVideoSource {
    fn resolution(&self) -> VideoResolution {
        self.inner.resolution()
    }

    fn next_frame(&mut self, stop: &PumpStop) -> Result<Option<BoxVideoFrame>, SourceError> {
        self.inner.next_frame(stop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::shader::gpu_available;

    fn test_config() -> DemoVideoSourceConfig {
        DemoVideoSourceConfig {
            resolution: VideoResolution { width: 64, height: 36 },
            framerate_fps: 1000,
        }
    }

    #[test]
    fn rejects_zero_configuration() {
        let mut config = test_config();
        config.resolution = VideoResolution::new(0, 36);
        assert!(DemoVideoSource::new_blocking(config).is_err());

        let mut config = test_config();
        config.framerate_fps = 0;
        assert!(DemoVideoSource::new_blocking(config).is_err());
    }

    #[test]
    fn yields_frames_with_configured_dimensions() {
        if !gpu_available() {
            eprintln!("skipping: no GPU adapter available");
            return;
        }
        let mut source = DemoVideoSource::new_blocking(test_config()).unwrap();

        let first = source.next_frame(&PumpStop::new()).unwrap().unwrap();
        let second = source.next_frame(&PumpStop::new()).unwrap().unwrap();
        assert_eq!((first.buffer.width(), first.buffer.height()), (64, 36));
        assert_eq!(first.timestamp_us, 0);
        assert_eq!(second.timestamp_us, 1_000);
    }
}
