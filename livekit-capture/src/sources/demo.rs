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

//! Solid-color demo source for testing.

use crate::{error::SourceError, pixel::PixelVideoSource, primitive::VideoResolution, pump::PumpStop};
use livekit::webrtc::video_frame::{BoxVideoFrame, I420Buffer, VideoFrame, VideoRotation};
use std::{
    thread,
    time::{Duration, Instant},
};
use thiserror::Error;

/// How long each palette color is shown before cycling to the next.
const COLOR_INTERVAL: Duration = Duration::from_millis(500);

/// Colors the demo source cycles through, as `(r, g, b)`.
const PALETTE: [(u8, u8, u8); 6] = [
    (0xE6, 0x32, 0x2E), // red
    (0xF4, 0x9D, 0x1A), // orange
    (0xF7, 0xD0, 0x38), // yellow
    (0x2E, 0xB8, 0x5C), // green
    (0x2E, 0x6F, 0xE6), // blue
    (0x8E, 0x44, 0xAD), // purple
];

/// Configuration for a [`DemoSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct DemoSourceConfig {
    /// Output resolution.
    pub resolution: VideoResolution,
    /// Output frame rate in frames per second.
    pub framerate_fps: u32,
}

/// Pixel video source that produces solid-color frames, cycling through a
/// fixed palette.
///
/// The source paces itself to the configured frame rate by sleeping and
/// never reaches end of stream; stop the pump driving it instead. It exists
/// to validate capture integration end to end without a device or pipeline
/// dependency.
#[derive(Debug)]
pub struct DemoSource {
    config: DemoSourceConfig,
    /// One `(y, u, v)` sample triple per palette color.
    colors: Vec<(u8, u8, u8)>,
    started: Option<Instant>,
    frame_index: u64,
}

impl DemoSource {
    /// Creates a demo source, rejecting an invalid configuration.
    pub fn new(config: DemoSourceConfig) -> Result<Self, SourceError> {
        let VideoResolution { width, height } = config.resolution;
        if width == 0 || height == 0 {
            return Err(SourceError::new(DemoSourceConfigError::ZeroResolution));
        }
        if config.framerate_fps == 0 {
            return Err(SourceError::new(DemoSourceConfigError::ZeroFramerate));
        }

        let colors = PALETTE.iter().map(|&color| yuv_from_rgb(color)).collect();
        Ok(Self { config, colors, started: None, frame_index: 0 })
    }

    fn frame_interval(&self) -> Duration {
        Duration::from_secs(1) / self.config.framerate_fps
    }
}

/// Error returned when a [`DemoSourceConfig`] cannot produce frames.
#[derive(Debug, Error)]
pub enum DemoSourceConfigError {
    /// The configured resolution has a zero component.
    #[error("demo source resolution must be non-zero")]
    ZeroResolution,
    /// The configured frame rate is zero.
    #[error("demo source frame rate must be non-zero")]
    ZeroFramerate,
}

impl PixelVideoSource for DemoSource {
    fn resolution(&self) -> VideoResolution {
        self.config.resolution
    }

    // Sleeps at most one frame interval, so the stop token is observed
    // promptly without integrating it into the wait.
    fn next_frame(&mut self, _stop: &PumpStop) -> Result<Option<BoxVideoFrame>, SourceError> {
        let started = *self.started.get_or_insert_with(Instant::now);

        // Pace against the ideal timeline so timestamps stay jitter-free.
        let interval_us = self.frame_interval().as_micros() as u64;
        let elapsed = Duration::from_micros(self.frame_index.saturating_mul(interval_us));
        let due = started + elapsed;
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }

        let timestamp_us = elapsed.as_micros() as i64;
        let color_index = (elapsed.as_micros() / COLOR_INTERVAL.as_micros()) as usize;
        let (y, u, v) = self.colors[color_index % self.colors.len()];

        self.frame_index += 1;
        let VideoResolution { width, height } = self.config.resolution;
        let mut buffer = I420Buffer::new(width, height);
        let (data_y, data_u, data_v) = buffer.data_mut();
        data_y.fill(y);
        data_u.fill(u);
        data_v.fill(v);

        Ok(Some(VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us,
            frame_metadata: None,
            buffer: Box::new(buffer),
        }))
    }
}

/// Converts an RGB color to limited-range BT.601 YUV.
fn yuv_from_rgb((r, g, b): (u8, u8, u8)) -> (u8, u8, u8) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let y = 16.0 + 65.481 * r + 128.553 * g + 24.966 * b;
    let u = 128.0 - 37.797 * r - 74.203 * g + 112.0 * b;
    let v = 128.0 + 112.0 * r - 93.786 * g - 18.214 * b;
    (y.round() as u8, u.round() as u8, v.round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> DemoSourceConfig {
        DemoSourceConfig {
            resolution: VideoResolution { width: 64, height: 36 },
            framerate_fps: 1000,
        }
    }

    #[test]
    fn yields_frames_with_configured_dimensions() {
        let mut source = DemoSource::new(test_config()).unwrap();

        let frame = source.next_frame(&PumpStop::new()).unwrap().unwrap();
        assert_eq!((frame.buffer.width(), frame.buffer.height()), (64, 36));

        let i420 = frame.buffer.as_i420().expect("demo source yields I420 buffers");
        let (y, u, v) = i420.data();
        assert_eq!(y.len(), 64 * 36);
        assert_eq!(u.len(), 32 * 18);
        assert_eq!(v.len(), 32 * 18);
    }

    #[test]
    fn timestamps_follow_the_frame_rate() {
        let mut source = DemoSource::new(test_config()).unwrap();

        let first = source.next_frame(&PumpStop::new()).unwrap().unwrap();
        let second = source.next_frame(&PumpStop::new()).unwrap().unwrap();
        assert_eq!(first.timestamp_us, 0);
        assert_eq!(second.timestamp_us, 1_000);
    }

    #[test]
    fn colors_cycle_at_the_color_interval() {
        // Two frames per color, so the first boundary lands on frame three.
        // The source paces itself in real time, so this trades frame count
        // for the ~COLOR_INTERVAL the test spends sleeping either way.
        let frame_interval = COLOR_INTERVAL / 2;
        let mut source = DemoSource::new(DemoSourceConfig {
            resolution: VideoResolution { width: 64, height: 36 },
            framerate_fps: (Duration::from_secs(1).as_micros() / frame_interval.as_micros()) as u32,
        })
        .unwrap();

        let luma = |frame: &BoxVideoFrame| frame.buffer.as_i420().unwrap().data().0[0];

        let first = source.next_frame(&PumpStop::new()).unwrap().unwrap();
        let same_color = source.next_frame(&PumpStop::new()).unwrap().unwrap();
        let next_color = source.next_frame(&PumpStop::new()).unwrap().unwrap();
        assert_eq!(luma(&first), luma(&same_color));
        assert_ne!(luma(&first), luma(&next_color));
    }

    #[test]
    fn converts_primaries_to_expected_luma() {
        // White has maximum luma and centered chroma in limited range.
        assert_eq!(yuv_from_rgb((255, 255, 255)), (235, 128, 128));
        // Black has minimum luma and centered chroma.
        assert_eq!(yuv_from_rgb((0, 0, 0)), (16, 128, 128));
    }
}
