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

//! Runtime-agnostic camera capture sources for the LiveKit Rust SDK.
//!
//! This crate provides a single [`Capture`] trait abstracting over
//! several backends so that publisher code can drive any of them with
//! identical logic. The currently supported backends are:
//!
//! - [`uvc`] (default feature): USB UVC webcams via `nokhwa`, producing
//!   I420 frames (with libyuv-backed YUYV/MJPEG conversion).
//! - [`libcamera_src`] (`libcamera` feature): Raspberry Pi CSI cameras
//!   via libcamera. Produces DMABUF-backed [`CaptureFrame::Native`]
//!   frames that the V4L2 hardware encoder can import zero-copy.
//!
//! A [`Publisher`] actor wraps a [`Capture`] implementation and feeds
//! frames into a [`NativeVideoSource`](libwebrtc::video_source::native::NativeVideoSource)
//! on a dedicated OS thread, so the same publishing logic works across
//! every backend.

use std::time::Duration;

use libwebrtc::video_frame::{native::NativeBuffer, I420Buffer};

mod error;
mod publisher;

#[cfg(feature = "uvc")]
pub mod uvc;

#[cfg(feature = "libcamera")]
pub mod libcamera_src;

pub use error::CaptureError;
pub use publisher::{CaptureHook, FrameContext, Publisher, PublisherConfig, PublisherStats};

/// Requested capture configuration.
///
/// Backends do their best to honour these values but may negotiate
/// alternatives (the actual values come back via [`StreamFormat`]).
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Zero-based index of the camera to use.
    pub camera_index: usize,
    /// Optional explicit device path (e.g. `/dev/video0`). When `None`,
    /// `camera_index` is used to pick a device.
    pub device_path: Option<String>,
    /// Requested frame width in pixels.
    pub width: u32,
    /// Requested frame height in pixels.
    pub height: u32,
    /// Requested frame rate in frames per second.
    pub fps: u32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self { camera_index: 0, device_path: None, width: 1280, height: 720, fps: 30 }
    }
}

/// Negotiated stream format reported by a [`Capture`] implementation.
#[derive(Debug, Clone, Copy)]
pub struct StreamFormat {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

/// A single captured frame. Either a CPU-resident I420 buffer or a
/// native (DMABUF) buffer, plus a best-effort wall-clock capture
/// timestamp in microseconds since the UNIX epoch.
pub enum CaptureFrame {
    I420 { buffer: I420Buffer, capture_ts_us: Option<u64> },
    Native { buffer: NativeBuffer, capture_ts_us: Option<u64> },
}

impl CaptureFrame {
    pub fn capture_ts_us(&self) -> Option<u64> {
        match self {
            CaptureFrame::I420 { capture_ts_us, .. } => *capture_ts_us,
            CaptureFrame::Native { capture_ts_us, .. } => *capture_ts_us,
        }
    }
}

/// Pluggable camera capture source.
///
/// Implementations are not required to be `Send + Sync`; the
/// [`Publisher`] runs the capture loop on a dedicated thread that
/// exclusively owns its `Capture`.
pub trait Capture: Send {
    /// Open the underlying device with the requested configuration and
    /// begin streaming. Returns the negotiated [`StreamFormat`] so the
    /// caller can pre-size their pipeline.
    fn start(&mut self, cfg: &CaptureConfig) -> Result<StreamFormat, CaptureError>;

    /// Block until the next frame is available (or `timeout` elapses).
    /// Returns `Ok(None)` on timeout.
    fn next_frame(&mut self, timeout: Duration) -> Result<Option<CaptureFrame>, CaptureError>;

    /// Stop streaming and release the device.
    fn stop(&mut self);
}
