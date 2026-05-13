// Copyright 2025 LiveKit, Inc.
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

//! Backend-agnostic publishing loop on top of [`Capture`].

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use libwebrtc::video_frame::{FrameMetadata, VideoFrame, VideoRotation};
use libwebrtc::video_source::native::NativeVideoSource;
use log::{debug, info, warn};

use crate::{Capture, CaptureConfig, CaptureError, CaptureFrame, StreamFormat};

/// Configuration for the publishing loop.
#[derive(Debug, Clone)]
pub struct PublisherConfig {
    pub capture: CaptureConfig,
    /// Attach `user_timestamp` (capture-wall-clock microseconds) to every
    /// frame's [`FrameMetadata`].
    pub attach_timestamp: bool,
    /// Attach a monotonically-increasing `frame_id` to every frame's
    /// [`FrameMetadata`].
    pub attach_frame_id: bool,
}

impl Default for PublisherConfig {
    fn default() -> Self {
        Self { capture: CaptureConfig::default(), attach_timestamp: false, attach_frame_id: false }
    }
}

/// Snapshot of the publisher's progress.
#[derive(Debug, Clone, Default)]
pub struct PublisherStats {
    /// Number of frames successfully delivered to the video source.
    pub frames_published: u64,
    /// Number of frames dropped (failed capture, conversion errors).
    pub frames_dropped: u64,
}

/// Frame hook invoked after each frame is captured but before it's
/// handed to the video source. Useful for in-place overlays (e.g.
/// burned-in timestamps).
pub type CaptureHook = Box<dyn FnMut(&mut CaptureFrame, FrameContext) + Send + 'static>;

/// Context passed to a [`CaptureHook`].
#[derive(Debug, Clone, Copy)]
pub struct FrameContext {
    /// Negotiated stream resolution / fps.
    pub format: StreamFormat,
    /// Monotonic frame counter starting at 1.
    pub frame_id: u32,
}

/// Long-running capture-to-publish actor.
///
/// Runs a [`Capture`] implementation on a dedicated OS thread and
/// forwards every produced frame to a [`NativeVideoSource`]. The thread
/// exits when [`Publisher::stop`] is called or when the [`Capture`]
/// returns an unrecoverable error.
pub struct Publisher {
    thread: Option<JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
    stats: Arc<PublisherStatsAtomic>,
    format: StreamFormat,
}

struct PublisherStatsAtomic {
    frames_published: AtomicU64,
    frames_dropped: AtomicU64,
}

impl Publisher {
    /// Build a new publisher, opening the camera before this returns.
    /// On success the loop is running.
    pub fn start<C: Capture + 'static>(
        mut capture: C,
        source: NativeVideoSource,
        cfg: PublisherConfig,
        hook: Option<CaptureHook>,
    ) -> Result<Self, CaptureError> {
        let format = capture.start(&cfg.capture)?;
        info!(
            "Publisher: capture opened at {}x{} @ {} fps",
            format.width, format.height, format.fps
        );

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(PublisherStatsAtomic {
            frames_published: AtomicU64::new(0),
            frames_dropped: AtomicU64::new(0),
        });

        let stop_flag_clone = stop_flag.clone();
        let stats_clone = stats.clone();
        let thread = thread::Builder::new()
            .name("livekit-capture-publisher".into())
            .spawn(move || {
                run_loop(capture, source, cfg, format, hook, stop_flag_clone, stats_clone);
            })
            .map_err(|e| CaptureError::DeviceUnavailable(format!("spawn publisher: {e}")))?;

        Ok(Self { thread: Some(thread), stop_flag, stats, format })
    }

    /// Negotiated stream format (the capture backend may adjust the
    /// requested resolution/fps).
    pub fn stream_format(&self) -> StreamFormat {
        self.format
    }

    /// Capture progress snapshot.
    pub fn stats(&self) -> PublisherStats {
        PublisherStats {
            frames_published: self.stats.frames_published.load(Ordering::Relaxed),
            frames_dropped: self.stats.frames_dropped.load(Ordering::Relaxed),
        }
    }

    /// Signal the publisher thread to exit, then join. Blocks until the
    /// thread has actually stopped.
    pub fn stop(mut self) {
        self.stop_flag.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Publisher {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn unix_time_us_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_micros() as u64).unwrap_or(0)
}

fn run_loop<C: Capture>(
    mut capture: C,
    source: NativeVideoSource,
    cfg: PublisherConfig,
    format: StreamFormat,
    mut hook: Option<CaptureHook>,
    stop_flag: Arc<AtomicBool>,
    stats: Arc<PublisherStatsAtomic>,
) {
    let frame_interval = Duration::from_secs_f64(1.0 / format.fps.max(1) as f64);
    let start_ts = Instant::now();
    let mut next_frame_at = Instant::now();
    let mut frame_counter: u32 = 1;

    while !stop_flag.load(Ordering::Acquire) {
        // Pace at the negotiated frame rate. The capture backend may
        // produce frames faster than this; the timeout below ensures we
        // don't busy-loop, and we skip publishing when we're early.
        let now = Instant::now();
        if now < next_frame_at {
            let sleep = next_frame_at - now;
            // Cap to a short slice so we still respond to shutdown
            // signals promptly.
            thread::sleep(sleep.min(Duration::from_millis(50)));
            continue;
        }
        next_frame_at += frame_interval;
        if next_frame_at < now {
            // Fell behind by more than a frame; resync.
            next_frame_at = now + frame_interval;
        }

        // Pull the next captured frame (block up to ~2 frame periods).
        let timeout = (frame_interval * 2).max(Duration::from_millis(100));
        let mut captured = match capture.next_frame(timeout) {
            Ok(Some(f)) => f,
            Ok(None) => {
                debug!("Publisher: capture timeout");
                continue;
            }
            Err(CaptureError::FrameRead(msg)) => {
                warn!("Publisher: frame read failed: {msg}; continuing");
                stats.frames_dropped.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            Err(e) => {
                warn!("Publisher: fatal capture error: {e}; exiting loop");
                break;
            }
        };

        if let Some(hook) = hook.as_mut() {
            hook(&mut captured, FrameContext { format, frame_id: frame_counter });
        }

        let capture_ts_us = captured.capture_ts_us().unwrap_or_else(unix_time_us_now);
        let user_ts = cfg.attach_timestamp.then_some(capture_ts_us);
        let fid = if cfg.attach_frame_id { Some(frame_counter) } else { None };
        let frame_metadata = (user_ts.is_some() || fid.is_some())
            .then_some(FrameMetadata { user_timestamp: user_ts, frame_id: fid });
        let timestamp_us = start_ts.elapsed().as_micros() as i64;

        // Hand the frame to NativeVideoSource. The two arms differ only
        // in the buffer payload; build a typed `VideoFrame` per arm so
        // each variant carries its concrete buffer type.
        match captured {
            CaptureFrame::I420 { buffer, .. } => {
                let frame = VideoFrame {
                    rotation: VideoRotation::VideoRotation0,
                    timestamp_us,
                    frame_metadata,
                    buffer,
                };
                source.capture_frame(&frame);
            }
            CaptureFrame::Native { buffer, .. } => {
                let frame = VideoFrame {
                    rotation: VideoRotation::VideoRotation0,
                    timestamp_us,
                    frame_metadata,
                    buffer,
                };
                source.capture_frame(&frame);
            }
        }

        stats.frames_published.fetch_add(1, Ordering::Relaxed);
        frame_counter = frame_counter.wrapping_add(1);
    }

    capture.stop();
    info!("Publisher: stopped");
}
