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

use std::{
    thread,
    time::{Duration, Instant},
};

use bytes::Bytes;

use crate::source::{
    PixelVideoData, PixelVideoFrame, PixelVideoSource, SourceError, VideoResolution, VideoSource,
};

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
pub struct DemoSourceConfig {
    /// Output resolution.
    pub resolution: VideoResolution,
    /// Output frame rate in frames per second.
    pub framerate_fps: u32,
    /// How long each palette color is shown before cycling to the next.
    pub color_interval: Duration,
}

impl Default for DemoSourceConfig {
    fn default() -> Self {
        Self {
            resolution: VideoResolution { width: 1280, height: 720 },
            framerate_fps: 30,
            color_interval: Duration::from_millis(500),
        }
    }
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
    /// One pre-rendered `(y, u, v)` plane set per palette color.
    planes: Vec<(Bytes, Bytes, Bytes)>,
    started: Option<Instant>,
    frame_index: u64,
}

impl DemoSource {
    /// Creates a demo source.
    ///
    /// # Panics
    ///
    /// Panics if the configured resolution or frame rate is zero, or if the
    /// color interval is shorter than one frame.
    pub fn new(config: DemoSourceConfig) -> Self {
        let VideoResolution { width, height } = config.resolution;
        assert!(width > 0 && height > 0, "demo source resolution must be non-zero");
        assert!(config.framerate_fps > 0, "demo source frame rate must be non-zero");
        assert!(
            config.color_interval >= Duration::from_secs(1) / config.framerate_fps,
            "demo source color interval must be at least one frame"
        );

        let luma_len = (width * height) as usize;
        let chroma_len = (width.div_ceil(2) * height.div_ceil(2)) as usize;
        let planes = PALETTE
            .iter()
            .map(|&color| {
                let (y, u, v) = yuv_from_rgb(color);
                (
                    Bytes::from(vec![y; luma_len]),
                    Bytes::from(vec![u; chroma_len]),
                    Bytes::from(vec![v; chroma_len]),
                )
            })
            .collect();

        Self { config, planes, started: None, frame_index: 0 }
    }

    fn frame_interval(&self) -> Duration {
        Duration::from_secs(1) / self.config.framerate_fps
    }
}

impl Default for DemoSource {
    fn default() -> Self {
        Self::new(DemoSourceConfig::default())
    }
}

impl PixelVideoSource for DemoSource {
    fn resolution(&self) -> VideoResolution {
        self.config.resolution
    }

    fn next_frame(&mut self) -> Result<Option<PixelVideoFrame>, SourceError> {
        let started = *self.started.get_or_insert_with(Instant::now);

        // Pace against the ideal timeline so timestamps stay jitter-free.
        let interval_us = self.frame_interval().as_micros() as u64;
        let elapsed = Duration::from_micros(self.frame_index.saturating_mul(interval_us));
        let due = started + elapsed;
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }

        let timestamp_us = elapsed.as_micros() as i64;
        let color_index =
            (elapsed.as_micros() / self.config.color_interval.as_micros().max(1)) as usize;
        let (y, u, v) = self.planes[color_index % self.planes.len()].clone();

        self.frame_index += 1;
        let VideoResolution { width, height } = self.config.resolution;
        Ok(Some(PixelVideoFrame {
            width,
            height,
            timestamp_us,
            data: PixelVideoData::I420 {
                y,
                u,
                v,
                stride_y: width,
                stride_u: width.div_ceil(2),
                stride_v: width.div_ceil(2),
            },
        }))
    }
}

impl From<DemoSource> for VideoSource {
    fn from(source: DemoSource) -> Self {
        Self::pixel(source)
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
            color_interval: Duration::from_millis(2),
        }
    }

    #[test]
    fn yields_frames_with_configured_dimensions() {
        let mut source = DemoSource::new(test_config());

        let frame = source.next_frame().unwrap().unwrap();
        assert_eq!((frame.width, frame.height), (64, 36));

        let PixelVideoData::I420 { y, u, v, .. } = &frame.data;
        assert_eq!(y.len(), 64 * 36);
        assert_eq!(u.len(), 32 * 18);
        assert_eq!(v.len(), 32 * 18);
    }

    #[test]
    fn timestamps_follow_the_frame_rate() {
        let mut source = DemoSource::new(test_config());

        let first = source.next_frame().unwrap().unwrap();
        let second = source.next_frame().unwrap().unwrap();
        assert_eq!(first.timestamp_us, 0);
        assert_eq!(second.timestamp_us, 1_000);
    }

    #[test]
    fn colors_cycle_at_the_color_interval() {
        let mut source = DemoSource::new(test_config());

        let luma = |frame: &PixelVideoFrame| {
            let PixelVideoData::I420 { y, .. } = &frame.data;
            y[0]
        };

        // Two frames per color at 1000 fps with a 2 ms interval.
        let first = source.next_frame().unwrap().unwrap();
        let same_color = source.next_frame().unwrap().unwrap();
        let next_color = source.next_frame().unwrap().unwrap();
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
