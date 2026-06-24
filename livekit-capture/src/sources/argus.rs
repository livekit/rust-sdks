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

//! NVIDIA Argus/libargus capture for Jetson MIPI CSI cameras.

use thiserror::Error;

#[cfg(livekit_capture_argus)]
use crate::device::{CaptureBackend, CaptureDeviceSelector};
use crate::{
    device::{
        CaptureDeviceInfo, CaptureFormat, CaptureFrameFormat, CapturePath, CaptureResolution,
    },
    dmabuf::DmaBufFrame,
};

#[cfg(livekit_capture_argus)]
use crate::dmabuf::{DmaBufPixelFormat, DmaBufPlane};
#[cfg(livekit_capture_argus)]
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[cfg(livekit_capture_argus)]
use std::{ffi::c_int, ffi::c_void};

#[cfg(livekit_capture_argus)]
extern "C" {
    fn lk_argus_create_session(
        sensor_index: c_int,
        width: c_int,
        height: c_int,
        fps: c_int,
    ) -> *mut c_void;

    fn lk_argus_destroy_session(session: *mut c_void);

    fn lk_argus_acquire_frame_with_metadata(
        session: *mut c_void,
        sensor_timestamp_ns: *mut u64,
        acquire_wait_ns: *mut u64,
        blit_ns: *mut u64,
    ) -> c_int;

    fn lk_argus_release_frame(session: *mut c_void);
}

/// Options used to open a Jetson Argus capture session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgusCaptureOptions {
    /// MIPI CSI sensor index.
    pub sensor_index: u32,
    /// Requested capture format.
    pub format: CaptureFormat,
}

impl ArgusCaptureOptions {
    /// Creates options for NV12 DMA-BUF capture from a Jetson MIPI CSI sensor.
    pub const fn new(sensor_index: u32, resolution: CaptureResolution, frame_rate: u32) -> Self {
        Self {
            sensor_index,
            format: CaptureFormat::new(resolution, frame_rate, CaptureFrameFormat::Nv12),
        }
    }
}

impl Default for ArgusCaptureOptions {
    fn default() -> Self {
        Self::new(0, CaptureResolution::new(1280, 720), 30)
    }
}

/// Error returned by the Argus capture backend.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ArgusError {
    /// Argus capture is not available for this target or build.
    #[error("libargus capture is not available on this target or build")]
    Unsupported,
    /// Argus only publishes NV12 DMA-BUF frames in this backend.
    #[error("libargus capture only supports NV12 DMA-BUF frames, got {0:?}")]
    UnsupportedFrameFormat(CaptureFrameFormat),
    /// The requested format contains an invalid value.
    #[error("invalid Argus capture option: {0}")]
    InvalidOption(&'static str),
    /// A numeric option could not be represented by the C shim.
    #[error("Argus capture option is out of range for the C shim: {0}")]
    OptionOutOfRange(&'static str),
    /// The C shim failed to create an Argus capture session.
    #[error("failed to create Argus capture session")]
    CreateSessionFailed,
    /// The C shim failed to acquire a frame.
    #[error("Argus frame acquisition failed")]
    AcquireFrameFailed,
}

/// One Argus frame backed by an NV12 DMA-BUF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgusFrame {
    /// DMA-BUF frame suitable for [`crate::VideoCaptureTrack::capture_dmabuf`].
    pub dmabuf: DmaBufFrame,
    /// Argus sensor start timestamp in nanoseconds, when available.
    pub sensor_timestamp_ns: Option<u64>,
    /// Argus sensor start timestamp translated to UNIX-epoch microseconds, when available.
    pub sensor_timestamp_us: Option<u64>,
    /// Time spent waiting for `FrameConsumer::acquireFrame` to return.
    pub acquire_wait_ns: u64,
    /// Time spent copying the acquired EGLStream frame into the DMA buffer.
    pub blit_ns: u64,
}

impl ArgusFrame {
    /// Returns the DMA-BUF frame descriptor.
    pub fn dmabuf_frame(&self) -> &DmaBufFrame {
        &self.dmabuf
    }
}

/// Jetson Argus capture session that emits NV12 DMA-BUF frames.
#[derive(Debug)]
pub struct ArgusCaptureSession {
    #[cfg(livekit_capture_argus)]
    handle: *mut c_void,
    options: ArgusCaptureOptions,
    #[cfg(livekit_capture_argus)]
    started_at: Instant,
}

// SAFETY: The C++ Argus session is driven by one mutable Rust owner at a time.
unsafe impl Send for ArgusCaptureSession {}

impl ArgusCaptureSession {
    /// Opens an Argus capture session.
    pub fn new(options: ArgusCaptureOptions) -> Result<Self, ArgusError> {
        validate_options(&options)?;
        Self::open(options)
    }

    /// Captures the next frame as an NV12 DMA-BUF.
    ///
    /// The returned DMA-BUF file descriptor is owned by the Argus session's
    /// internal buffer ring. It remains valid until the session is dropped, but
    /// callers should publish frames promptly so the ring can be reused.
    pub fn capture_frame(&mut self) -> Result<ArgusFrame, ArgusError> {
        self.acquire_frame_inner()
    }

    /// Acquires the next captured frame as an NV12 DMA-BUF.
    #[deprecated(note = "use capture_frame")]
    pub fn acquire_frame(&mut self) -> Result<ArgusFrame, ArgusError> {
        self.capture_frame()
    }

    /// Releases the currently held Argus frame, when one is held by the shim.
    pub fn release_frame(&mut self) {
        self.release_frame_inner();
    }

    /// Returns the configured frame width.
    pub fn width(&self) -> u32 {
        self.options.format.resolution.width
    }

    /// Returns the configured frame height.
    pub fn height(&self) -> u32 {
        self.options.format.resolution.height
    }

    /// Returns the requested capture format.
    pub fn format(&self) -> CaptureFormat {
        self.options.format
    }

    /// Returns the configured capture options.
    pub fn options(&self) -> &ArgusCaptureOptions {
        &self.options
    }

    /// Returns the capture path produced by this session.
    pub fn capture_path(&self) -> CapturePath {
        CapturePath::DmaBuf
    }

    #[cfg(livekit_capture_argus)]
    fn open(options: ArgusCaptureOptions) -> Result<Self, ArgusError> {
        let sensor_index = c_int_from_u32(options.sensor_index, "sensor_index")?;
        let width = c_int_from_u32(options.format.resolution.width, "width")?;
        let height = c_int_from_u32(options.format.resolution.height, "height")?;
        let frame_rate = c_int_from_u32(options.format.frame_rate, "frame_rate")?;

        let handle = unsafe {
            // SAFETY: The C shim expects plain integer values and returns either
            // a valid opaque session pointer or null on failure.
            lk_argus_create_session(sensor_index, width, height, frame_rate)
        };
        if handle.is_null() {
            return Err(ArgusError::CreateSessionFailed);
        }

        Ok(Self { handle, options, started_at: Instant::now() })
    }

    #[cfg(not(livekit_capture_argus))]
    fn open(_options: ArgusCaptureOptions) -> Result<Self, ArgusError> {
        Err(ArgusError::Unsupported)
    }

    #[cfg(livekit_capture_argus)]
    fn acquire_frame_inner(&mut self) -> Result<ArgusFrame, ArgusError> {
        let mut sensor_timestamp_ns = 0;
        let mut acquire_wait_ns = 0;
        let mut blit_ns = 0;
        let fd = unsafe {
            // SAFETY: `self.handle` is created by `lk_argus_create_session` and
            // remains valid until `Drop`; the out-pointers are valid for the call.
            lk_argus_acquire_frame_with_metadata(
                self.handle,
                &mut sensor_timestamp_ns,
                &mut acquire_wait_ns,
                &mut blit_ns,
            )
        };
        if fd < 0 {
            return Err(ArgusError::AcquireFrameFailed);
        }

        let sensor_timestamp_ns = (sensor_timestamp_ns > 0).then_some(sensor_timestamp_ns);
        let sensor_timestamp_us = sensor_timestamp_ns.and_then(sensor_wall_time_us);
        let resolution = self.options.format.resolution;
        let dmabuf = DmaBufFrame {
            width: resolution.width,
            height: resolution.height,
            pixel_format: DmaBufPixelFormat::Nv12,
            planes: vec![DmaBufPlane { fd, offset: 0, stride: resolution.width }],
            modifier: None,
            timestamp_us: elapsed_us(self.started_at.elapsed()),
            sensor_timestamp_us,
        };

        Ok(ArgusFrame {
            dmabuf,
            sensor_timestamp_ns,
            sensor_timestamp_us,
            acquire_wait_ns,
            blit_ns,
        })
    }

    #[cfg(not(livekit_capture_argus))]
    fn acquire_frame_inner(&mut self) -> Result<ArgusFrame, ArgusError> {
        Err(ArgusError::Unsupported)
    }

    #[cfg(livekit_capture_argus)]
    fn release_frame_inner(&mut self) {
        unsafe {
            // SAFETY: `self.handle` is owned by this session and valid until `Drop`.
            lk_argus_release_frame(self.handle);
        }
    }

    #[cfg(not(livekit_capture_argus))]
    fn release_frame_inner(&mut self) {}
}

impl Drop for ArgusCaptureSession {
    fn drop(&mut self) {
        #[cfg(livekit_capture_argus)]
        if !self.handle.is_null() {
            unsafe {
                // SAFETY: `self.handle` is owned by this session and is destroyed once here.
                lk_argus_destroy_session(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}

fn validate_options(options: &ArgusCaptureOptions) -> Result<(), ArgusError> {
    if options.format.frame_format != CaptureFrameFormat::Nv12 {
        return Err(ArgusError::UnsupportedFrameFormat(options.format.frame_format));
    }
    if options.format.resolution.width == 0 {
        return Err(ArgusError::InvalidOption("width must be non-zero"));
    }
    if options.format.resolution.height == 0 {
        return Err(ArgusError::InvalidOption("height must be non-zero"));
    }
    if options.format.frame_rate == 0 {
        return Err(ArgusError::InvalidOption("frame_rate must be non-zero"));
    }
    Ok(())
}

/// Returns Jetson Argus capture devices.
pub fn devices() -> Result<Vec<CaptureDeviceInfo>, ArgusError> {
    #[cfg(livekit_capture_argus)]
    {
        return Ok(vec![CaptureDeviceInfo {
            backend: CaptureBackend::LibArgus,
            id: "0".to_string(),
            selector: CaptureDeviceSelector::Index(0),
            name: "Jetson Argus sensor 0".to_string(),
            model_id: None,
            manufacturer: Some("NVIDIA".to_string()),
            paths: vec![CapturePath::DmaBuf],
            formats: vec![ArgusCaptureOptions::default().format],
            formats_complete: false,
        }]);
    }
    #[cfg(not(livekit_capture_argus))]
    {
        Err(ArgusError::Unsupported)
    }
}

#[cfg(livekit_capture_argus)]
fn c_int_from_u32(value: u32, field: &'static str) -> Result<c_int, ArgusError> {
    c_int::try_from(value).map_err(|_| ArgusError::OptionOutOfRange(field))
}

#[cfg(livekit_capture_argus)]
fn elapsed_us(duration: Duration) -> i64 {
    i64::try_from(duration.as_micros()).unwrap_or(i64::MAX)
}

#[cfg(livekit_capture_argus)]
fn sensor_wall_time_us(sensor_timestamp_ns: u64) -> Option<u64> {
    let wall_time_us = unix_time_us_now()?;
    sensor_monotonic_ns_to_unix_us(sensor_timestamp_ns, wall_time_us)
}

/// Converts an Argus `CLOCK_MONOTONIC` timestamp into a UNIX-epoch microsecond timestamp.
pub fn sensor_monotonic_ns_to_unix_us(sensor_timestamp_ns: u64, wall_time_us: u64) -> Option<u64> {
    let monotonic_now_ns = monotonic_time_ns_now()?;
    let monotonic_delta_us = monotonic_now_ns.abs_diff(sensor_timestamp_ns) / 1_000;
    if sensor_timestamp_ns <= monotonic_now_ns {
        Some(wall_time_us.saturating_sub(monotonic_delta_us))
    } else {
        Some(wall_time_us.saturating_add(monotonic_delta_us))
    }
}

#[cfg(livekit_capture_argus)]
fn unix_time_us_now() -> Option<u64> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(elapsed.as_micros()).ok()
}

#[cfg(target_os = "linux")]
fn monotonic_time_ns_now() -> Option<u64> {
    #[repr(C)]
    struct Timespec {
        tv_sec: i64,
        tv_nsec: i64,
    }

    extern "C" {
        fn clock_gettime(clk_id: i32, tp: *mut Timespec) -> i32;
    }

    const CLOCK_MONOTONIC: i32 = 1;
    let mut ts = Timespec { tv_sec: 0, tv_nsec: 0 };
    let ret = unsafe {
        // SAFETY: `ts` is a valid writable `Timespec` for the duration of the call.
        clock_gettime(CLOCK_MONOTONIC, &mut ts)
    };
    if ret != 0 || ts.tv_sec < 0 || ts.tv_nsec < 0 {
        return None;
    }

    let seconds = u64::try_from(ts.tv_sec).ok()?;
    let nanos = u64::try_from(ts.tv_nsec).ok()?;
    seconds.checked_mul(1_000_000_000)?.checked_add(nanos)
}

#[cfg(not(target_os = "linux"))]
fn monotonic_time_ns_now() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_nv12_only() {
        let mut options = ArgusCaptureOptions::default();
        options.format.frame_format = CaptureFrameFormat::I420;
        let err = ArgusCaptureSession::new(options).expect_err("I420 must be rejected");
        assert_eq!(err, ArgusError::UnsupportedFrameFormat(CaptureFrameFormat::I420));
    }

    #[test]
    fn validates_non_zero_frame_rate() {
        let options = ArgusCaptureOptions::new(0, CaptureResolution::new(1280, 720), 0);
        let err = ArgusCaptureSession::new(options).expect_err("zero frame rate must be rejected");
        assert_eq!(err, ArgusError::InvalidOption("frame_rate must be non-zero"));
    }
}
