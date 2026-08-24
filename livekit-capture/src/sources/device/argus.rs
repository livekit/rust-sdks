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

//! NVIDIA Jetson CSI capture backend using libargus.
//!
//! Frames come out of the hardware ISP as NV12 DMA buffers and are wrapped
//! as native video buffers, so they reach the RTC track — and the Jetson
//! hardware encoder — without a CPU copy. Only [`DeviceFrameFormat::Nv12`]
//! is deliverable.
//!
//! The shim (see the `libargus-sys` crate) blits each acquired frame into a
//! slot of a fixed DMA-buffer ring and leases that slot out until the frame
//! is released. The release hook on each published frame returns the slot
//! once the WebRTC pipeline drops its last reference, so a frame stays valid
//! however long the encoder holds it; a sustained encoder backlog surfaces
//! as ring exhaustion (a retryable condition), not corruption.
//!
//! At runtime this backend requires a Jetson device with the Argus stack
//! (`nvargus-daemon`) running. Everywhere else the shim reports itself
//! unavailable and this backend degrades to "no devices".

use std::{
    ffi::{c_char, c_void, CStr},
    ptr::NonNull,
    sync::{Arc, Mutex, Once},
    thread,
    time::{Duration, Instant},
};

use libargus_sys as sys;
use livekit::webrtc::video_frame::{
    native::{remove_dmabuf_surface_cache_entry, DmaBufPixelFormat, NativeBuffer},
    BoxVideoFrame, VideoFrame, VideoRotation,
};

use super::timestamp::{
    clock_time, elapsed_us, monotonic_timestamp_to_wallclock, select_capture_wall_time_us,
    unix_time_us_now,
};
use super::{
    capture_frame_metadata, DeviceFormat, DeviceFormatRequest, DeviceFrameFormat, DeviceInfo,
    DeviceVideoSourceConfig, DeviceVideoSourceError,
};
use crate::{primitive::VideoResolution, pump::PumpStop};

/// How long opening a session may wait for the first frame.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(5);

/// How long one frame acquire may block before the stop token is rechecked.
/// Acquire timeouts are harmless in the shim's mailbox mode, so this bounds
/// stop latency the same way the V4L2 backend's poll interval does.
const ACQUIRE_TIMEOUT_NS: u64 = 100_000_000;

/// Backoff before retrying when every ring slot is leased (the shim returns
/// ring exhaustion immediately, so retrying without a pause would spin).
const RING_EXHAUSTED_BACKOFF: Duration = Duration::from_millis(5);

/// Default format delivered for [`DeviceFormatRequest::Default`], when the
/// sensor covers it.
const DEFAULT_RESOLUTION: VideoResolution = VideoResolution::new(1280, 720);
const DEFAULT_FRAMERATE_FPS: u32 = 30;

/// Tolerance when comparing frame durations. Sensor durations are reported
/// in nanoseconds and are often off by 1 ns from the ideal value (e.g.
/// 33333334 vs 33333333 for 30 fps).
const FRAME_DURATION_TOLERANCE_NS: u64 = 1_000_000;

const NANOS_PER_SECOND: u64 = 1_000_000_000;

/// Returns whether the backend can deliver this frame format. The DMA buffer
/// ring is NV12; nothing else is produced.
fn is_supported_source_format(frame_format: DeviceFrameFormat) -> bool {
    frame_format == DeviceFrameFormat::Nv12
}

/// Owner of the native shim session.
///
/// Shared between the [`Session`] and the release hook of every in-flight
/// frame, so the native session (and with it every leased DMA buffer fd)
/// outlives the last published frame even if the [`Session`] is dropped
/// first.
struct ShimSession {
    handle: NonNull<sys::LkArgusSession>,
    /// DMA buffer fds observed on acquired frames, for fd-to-surface cache
    /// eviction once the buffers are destroyed.
    seen_fds: Mutex<Vec<i32>>,
}

// SAFETY: The handle is only used from one consumer at a time for acquire
// (guarded by `Session::next_frame(&mut self)`), and the only entry points
// reachable through clones held by frame release hooks —
// `lk_argus_frame_release` and `lk_argus_session_destroy` — are documented
// as callable from any thread by the shim.
unsafe impl Send for ShimSession {}
// SAFETY: See above; `&ShimSession` only exposes thread-safe shim calls.
unsafe impl Sync for ShimSession {}

impl Drop for ShimSession {
    fn drop(&mut self) {
        // SAFETY: The handle is valid until this drop, and every frame lease
        // has been released (each holds a clone of the owning Arc).
        unsafe { sys::lk_argus_session_destroy(self.handle.as_ptr()) };

        // The DMA buffers are gone; fd numbers will be recycled by the OS.
        let seen_fds = self.seen_fds.get_mut().map(std::mem::take).unwrap_or_default();
        for fd in seen_fds {
            remove_dmabuf_surface_cache_entry(fd);
        }
    }
}

/// Argus capture session satisfying the backend contract, opened via
/// [`Session::open`] with a sensor index routed by the Linux dispatcher.
pub(super) struct Session {
    shim: Arc<ShimSession>,
    format: DeviceFormat,
    started_at: Instant,
    // Frame pulled while opening the session, handed out first.
    pending_frame: Option<BoxVideoFrame>,
    // Rate-limits ring-exhaustion warnings to state transitions.
    ring_exhausted: bool,
}

impl Session {
    /// Opens the sensor, negotiates the capture format against its sensor
    /// modes, and starts the capture by pulling the first frame.
    pub(super) fn open(
        sensor_index: u32,
        config: &DeviceVideoSourceConfig,
    ) -> Result<Self, DeviceVideoSourceError> {
        super::validate_config(config, is_supported_source_format)?;
        install_shim_logger();

        let modes = enumerate_modes(sensor_index)?;
        let (format, sensor_mode) = negotiate_format(&config.format, &modes)?;

        let shim_config = sys::LkArgusSessionConfig {
            device_index: i32::try_from(sensor_index)
                .map_err(|_| DeviceVideoSourceError::DeviceNotFound)?,
            sensor_mode_index: sensor_mode.index,
            width: i32::try_from(format.resolution.width)
                .map_err(|_| DeviceVideoSourceError::InvalidConfig("width exceeds range"))?,
            height: i32::try_from(format.resolution.height)
                .map_err(|_| DeviceVideoSourceError::InvalidConfig("height exceeds range"))?,
            fps: i32::try_from(format.framerate_fps)
                .map_err(|_| DeviceVideoSourceError::InvalidConfig("framerate exceeds range"))?,
            num_dma_bufs: 0, // shim default
        };

        let mut handle: *mut sys::LkArgusSession = std::ptr::null_mut();
        // SAFETY: Both pointers reference valid stack locations.
        let status = unsafe { sys::lk_argus_session_create(&shim_config, &mut handle) };
        if status != sys::LK_ARGUS_OK {
            return Err(map_shim_error(status, "opening capture session"));
        }
        let handle = NonNull::new(handle).ok_or_else(|| {
            DeviceVideoSourceError::Backend("argus shim returned a null session".to_string())
        })?;

        let mut session = Self {
            shim: Arc::new(ShimSession { handle, seen_fds: Mutex::new(Vec::new()) }),
            format,
            started_at: Instant::now(),
            pending_frame: None,
            ring_exhausted: false,
        };

        // Pull the first frame during construction: it proves the pipeline
        // delivers at the negotiated format and matches the facade contract
        // that open() fails fast on a dead camera.
        let deadline = Instant::now() + FIRST_FRAME_TIMEOUT;
        let first_frame = loop {
            match session.acquire_frame() {
                Ok(Some(frame)) => break frame,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        return Err(DeviceVideoSourceError::FrameTimeout);
                    }
                }
                Err(err) => return Err(err),
            }
        };
        session.pending_frame = Some(first_frame);

        log::info!(
            "Opened device \"argus:{}\": {} (sensor mode {}, NV12 DMA buffers, zero copy)",
            sensor_index,
            session.format,
            sensor_mode.index,
        );
        Ok(session)
    }

    /// Returns the negotiated capture format.
    pub(super) fn format(&self) -> DeviceFormat {
        self.format
    }

    /// Blocks until the next frame is available, returning `Ok(None)` once
    /// the stop token fires.
    pub(super) fn next_frame(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<BoxVideoFrame>, DeviceVideoSourceError> {
        if let Some(frame) = self.pending_frame.take() {
            return Ok(Some(frame));
        }

        // Bounded acquire timeouts keep the stop token observed within
        // ~ACQUIRE_TIMEOUT_NS even when the sensor stalls. Timeouts are
        // retryable: the shim's mailbox-mode stream always holds the latest
        // frame, so nothing is lost or half-consumed.
        loop {
            if stop.is_stopped() {
                return Ok(None);
            }
            if let Some(frame) = self.acquire_frame()? {
                return Ok(Some(frame));
            }
        }
    }

    /// Acquires one frame, returning `Ok(None)` on a retryable condition
    /// (acquire timeout or exhausted buffer ring).
    fn acquire_frame(&mut self) -> Result<Option<BoxVideoFrame>, DeviceVideoSourceError> {
        let fallback_wall_time_us = unix_time_us_now().unwrap_or_default();

        let mut frame = sys::LkArgusFrame::default();
        // SAFETY: The session handle is valid and this is the only consumer
        // thread; `frame` is a valid out pointer.
        let status = unsafe {
            sys::lk_argus_frame_acquire(self.shim.handle.as_ptr(), ACQUIRE_TIMEOUT_NS, &mut frame)
        };
        match status {
            sys::LK_ARGUS_OK => {}
            sys::LK_ARGUS_ERR_TIMEOUT => return Ok(None),
            sys::LK_ARGUS_ERR_NO_FREE_BUFFER => {
                if !self.ring_exhausted {
                    self.ring_exhausted = true;
                    log::warn!(
                        "Argus DMA buffer ring exhausted; the encoder is holding every \
                         in-flight frame (backpressure)"
                    );
                }
                thread::sleep(RING_EXHAUSTED_BACKOFF);
                return Ok(None);
            }
            error => return Err(map_shim_error(error, "acquiring frame")),
        }
        if self.ring_exhausted {
            self.ring_exhausted = false;
            log::info!("Argus DMA buffer ring recovered");
        }

        if let Ok(mut seen_fds) = self.shim.seen_fds.lock() {
            if !seen_fds.contains(&frame.dmabuf_fd) {
                seen_fds.push(frame.dmabuf_fd);
            }
        }

        let read_wall_time_us = unix_time_us_now().unwrap_or(fallback_wall_time_us);
        let backend_capture_timestamp = sensor_timestamp_to_wallclock(frame.sensor_timestamp_ns);
        let capture_wall_time_us = select_capture_wall_time_us(
            backend_capture_timestamp,
            fallback_wall_time_us,
            read_wall_time_us,
        );

        let shim = Arc::clone(&self.shim);
        let slot = frame.buffer_index;
        // SAFETY: The fd describes a leased NV12 ring slot the shim keeps
        // valid until it is released; the release hook returns the lease and
        // its captured Arc keeps the native session (and the fd) alive until
        // then. `lk_argus_frame_release` is callable from any thread.
        let buffer = unsafe {
            NativeBuffer::from_dmabuf(
                frame.dmabuf_fd,
                frame.width,
                frame.height,
                DmaBufPixelFormat::Nv12,
                move || {
                    // SAFETY: See above.
                    unsafe { sys::lk_argus_frame_release(shim.handle.as_ptr(), slot) };
                },
            )
        };

        Ok(Some(VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: elapsed_us(self.started_at.elapsed()),
            frame_metadata: Some(capture_frame_metadata(capture_wall_time_us)),
            buffer: Box::new(buffer),
        }))
    }
}

/// Returns the number of Argus sensors, or 0 when the shim is unavailable or
/// enumeration fails.
pub(super) fn sensor_count() -> u32 {
    if !sys::AVAILABLE {
        return 0;
    }
    install_shim_logger();
    // SAFETY: No pointer arguments.
    let count = unsafe { sys::lk_argus_device_count() };
    u32::try_from(count).unwrap_or(0)
}

/// Lists Argus CSI sensors. Empty — not an error — when the shim is
/// unavailable or reports no cameras, so enumeration can degrade to V4L2.
pub(super) fn devices() -> Result<Vec<DeviceInfo>, DeviceVideoSourceError> {
    let mut devices = Vec::new();
    for sensor_index in 0..sensor_count() {
        let mut info = sys::LkArgusDeviceInfo::default();
        // SAFETY: `info` is a valid out pointer.
        let status = unsafe {
            sys::lk_argus_device_info(sensor_index as i32, &mut info)
        };
        if status != sys::LK_ARGUS_OK {
            log::debug!(
                "Skipping Argus sensor {sensor_index}: {}",
                sys::lk_argus_status_string(status)
            );
            continue;
        }

        let name = c_chars_to_string(&info.name);
        let uuid = c_chars_to_string(&info.uuid);
        let formats = enumerate_modes(sensor_index)
            .map(|modes| mode_formats(&modes))
            .unwrap_or_default();

        devices.push(DeviceInfo {
            id: format!("argus:{sensor_index}"),
            name: if name.is_empty() { format!("CSI camera {sensor_index}") } else { name },
            model_id: Some(uuid).filter(|value| !value.is_empty()),
            manufacturer: Some("nvidia-argus".to_string()),
            formats,
            // The ISP scales to arbitrary output resolutions and any frame
            // rate within a mode's duration range; the list is
            // representative, not exhaustive.
            formats_complete: false,
        });
    }
    Ok(devices)
}

/// One Argus sensor mode, as negotiated against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SensorMode {
    index: i32,
    resolution: VideoResolution,
    min_frame_duration_ns: u64,
    max_frame_duration_ns: u64,
}

impl SensorMode {
    /// Highest whole frame rate the mode supports.
    fn max_framerate_fps(&self) -> u32 {
        if self.min_frame_duration_ns == 0 {
            return 0;
        }
        let fps =
            (NANOS_PER_SECOND + self.min_frame_duration_ns / 2) / self.min_frame_duration_ns;
        u32::try_from(fps).unwrap_or(u32::MAX)
    }

    /// Whether the mode's duration range covers a frame rate.
    fn supports_framerate(&self, framerate_fps: u32) -> bool {
        if framerate_fps == 0 {
            return false;
        }
        let requested_duration_ns = NANOS_PER_SECOND / u64::from(framerate_fps);
        self.min_frame_duration_ns <= requested_duration_ns + FRAME_DURATION_TOLERANCE_NS
            && requested_duration_ns <= self.max_frame_duration_ns + FRAME_DURATION_TOLERANCE_NS
    }

    /// Whether the mode's sensor resolution covers (is at least) the
    /// requested output resolution, which the ISP then scales down to.
    fn covers_resolution(&self, resolution: VideoResolution) -> bool {
        self.resolution.width >= resolution.width && self.resolution.height >= resolution.height
    }

    fn pixels(&self) -> u64 {
        u64::from(self.resolution.width) * u64::from(self.resolution.height)
    }
}

/// Reads a sensor's modes through the shim.
fn enumerate_modes(sensor_index: u32) -> Result<Vec<SensorMode>, DeviceVideoSourceError> {
    let sensor_index = i32::try_from(sensor_index)
        .map_err(|_| DeviceVideoSourceError::DeviceNotFound)?;

    let mut info = sys::LkArgusDeviceInfo::default();
    // SAFETY: `info` is a valid out pointer.
    let status = unsafe { sys::lk_argus_device_info(sensor_index, &mut info) };
    if status != sys::LK_ARGUS_OK {
        return Err(map_shim_error(status, "querying sensor"));
    }

    let mut modes = Vec::new();
    for mode_index in 0..info.sensor_mode_count {
        let mut mode = sys::LkArgusSensorModeInfo::default();
        // SAFETY: `mode` is a valid out pointer.
        let status = unsafe { sys::lk_argus_sensor_mode_info(sensor_index, mode_index, &mut mode) };
        if status != sys::LK_ARGUS_OK {
            log::debug!(
                "Skipping sensor mode {mode_index}: {}",
                sys::lk_argus_status_string(status)
            );
            continue;
        }
        if mode.width == 0 || mode.height == 0 || mode.min_frame_duration_ns == 0 {
            continue;
        }
        modes.push(SensorMode {
            index: mode_index,
            resolution: VideoResolution::new(mode.width, mode.height),
            min_frame_duration_ns: mode.min_frame_duration_ns,
            max_frame_duration_ns: mode.max_frame_duration_ns,
        });
    }
    Ok(modes)
}

/// Builds the representative `DeviceInfo` format list for a sensor: each
/// mode at its highest frame rate, plus common lower rates the mode covers.
fn mode_formats(modes: &[SensorMode]) -> Vec<DeviceFormat> {
    const COMMON_FRAMERATES_FPS: [u32; 3] = [15, 30, 60];

    let mut formats = Vec::new();
    for mode in modes {
        let mut push = |framerate_fps: u32| {
            let format =
                DeviceFormat::new(mode.resolution, framerate_fps, DeviceFrameFormat::Nv12);
            if !formats.contains(&format) {
                formats.push(format);
            }
        };
        push(mode.max_framerate_fps());
        for framerate_fps in COMMON_FRAMERATES_FPS {
            if mode.supports_framerate(framerate_fps) {
                push(framerate_fps);
            }
        }
    }
    formats
}

/// Negotiates the delivered format and the sensor mode to run it on.
///
/// The delivered resolution is the *requested* resolution whenever any mode
/// covers it — the ISP scales the stream — so `Exact` succeeds even when no
/// sensor mode matches it exactly. Mode choice follows the smallest covering
/// mode to keep sensor bandwidth (and power) down.
fn negotiate_format(
    request: &DeviceFormatRequest,
    modes: &[SensorMode],
) -> Result<(DeviceFormat, SensorMode), DeviceVideoSourceError> {
    if modes.is_empty() {
        return Err(DeviceVideoSourceError::Backend(
            "camera reports no usable sensor modes".to_string(),
        ));
    }

    let smallest_covering = |resolution: VideoResolution, framerate_fps: u32| {
        modes
            .iter()
            .filter(|mode| {
                mode.covers_resolution(resolution) && mode.supports_framerate(framerate_fps)
            })
            .min_by_key(|mode| mode.pixels())
            .copied()
    };

    match request {
        DeviceFormatRequest::Default => {
            if let Some(mode) = smallest_covering(DEFAULT_RESOLUTION, DEFAULT_FRAMERATE_FPS) {
                let format = DeviceFormat::new(
                    DEFAULT_RESOLUTION,
                    DEFAULT_FRAMERATE_FPS,
                    DeviceFrameFormat::Nv12,
                );
                return Ok((format, mode));
            }
            // Sensor smaller or slower than the default: fall back to the
            // largest mode at its own maximum frame rate.
            let mode = modes.iter().max_by_key(|mode| mode.pixels()).copied().unwrap();
            let format = DeviceFormat::new(
                mode.resolution,
                mode.max_framerate_fps(),
                DeviceFrameFormat::Nv12,
            );
            Ok((format, mode))
        }
        DeviceFormatRequest::Exact(requested) => {
            let mode = smallest_covering(requested.resolution, requested.framerate_fps)
                .ok_or(DeviceVideoSourceError::UnsupportedFormat(*requested))?;
            Ok((*requested, mode))
        }
        DeviceFormatRequest::Closest(requested) => {
            if let Some(mode) = smallest_covering(requested.resolution, requested.framerate_fps) {
                return Ok((*requested, mode));
            }
            // Nothing covers the request: clamp to the closest mode by
            // resolution distance, then clamp the frame rate to what that
            // mode supports.
            let mode = modes
                .iter()
                .min_by_key(|mode| resolution_distance(mode.resolution, requested.resolution))
                .copied()
                .unwrap();
            let resolution = VideoResolution::new(
                requested.resolution.width.min(mode.resolution.width),
                requested.resolution.height.min(mode.resolution.height),
            );
            let framerate_fps = requested.framerate_fps.min(mode.max_framerate_fps());
            Ok((DeviceFormat::new(resolution, framerate_fps, DeviceFrameFormat::Nv12), mode))
        }
        DeviceFormatRequest::HighestFramerate { resolution, frame_format: _ } => {
            let candidates = modes
                .iter()
                .filter(|mode| resolution.is_none_or(|res| mode.covers_resolution(res)));
            let mode = candidates
                .min_by_key(|mode| mode.min_frame_duration_ns)
                .copied()
                .ok_or_else(|| unsupported_constraint(*resolution, None))?;
            let delivered_resolution = resolution.unwrap_or(mode.resolution);
            let format = DeviceFormat::new(
                delivered_resolution,
                mode.max_framerate_fps(),
                DeviceFrameFormat::Nv12,
            );
            Ok((format, mode))
        }
        DeviceFormatRequest::HighestResolution { framerate_fps, frame_format: _ } => {
            let candidates = modes
                .iter()
                .filter(|mode| framerate_fps.is_none_or(|fps| mode.supports_framerate(fps)));
            let mode = candidates
                .max_by_key(|mode| mode.pixels())
                .copied()
                .ok_or_else(|| unsupported_constraint(None, *framerate_fps))?;
            let format = DeviceFormat::new(
                mode.resolution,
                framerate_fps.unwrap_or_else(|| mode.max_framerate_fps()),
                DeviceFrameFormat::Nv12,
            );
            Ok((format, mode))
        }
    }
}

/// Error for an unsatisfiable constrained request, expressed as the closest
/// concrete format for the error message.
fn unsupported_constraint(
    resolution: Option<VideoResolution>,
    framerate_fps: Option<u32>,
) -> DeviceVideoSourceError {
    DeviceVideoSourceError::UnsupportedFormat(DeviceFormat::new(
        resolution.unwrap_or(VideoResolution::new(0, 0)),
        framerate_fps.unwrap_or(0),
        DeviceFrameFormat::Nv12,
    ))
}

/// Squared euclidean distance between resolutions, for closest-match
/// selection.
fn resolution_distance(a: VideoResolution, b: VideoResolution) -> u64 {
    let dw = i64::from(a.width) - i64::from(b.width);
    let dh = i64::from(a.height) - i64::from(b.height);
    (dw * dw + dh * dh) as u64
}

/// Rebases the Argus sensor timestamp (`CLOCK_MONOTONIC` domain) onto the
/// wall clock.
fn sensor_timestamp_to_wallclock(sensor_timestamp_ns: u64) -> Option<Duration> {
    if sensor_timestamp_ns == 0 {
        return None;
    }
    let monotonic_now = clock_time(libc::CLOCK_MONOTONIC)?;
    let wall_now = clock_time(libc::CLOCK_REALTIME)?;
    monotonic_timestamp_to_wallclock(
        Duration::from_nanos(sensor_timestamp_ns),
        monotonic_now,
        wall_now,
    )
}

/// Maps a shim status code to a backend error.
fn map_shim_error(status: i32, op: &str) -> DeviceVideoSourceError {
    match status {
        // An unavailable shim behaves like a machine without the sensor.
        sys::LK_ARGUS_ERR_UNAVAILABLE | sys::LK_ARGUS_ERR_NO_DEVICE => {
            DeviceVideoSourceError::DeviceNotFound
        }
        sys::LK_ARGUS_ERR_TIMEOUT => DeviceVideoSourceError::FrameTimeout,
        _ => DeviceVideoSourceError::Backend(format!(
            "argus error while {op}: {}",
            sys::lk_argus_status_string(status)
        )),
    }
}

/// Converts a NUL-terminated C character array to a `String`.
fn c_chars_to_string(chars: &[c_char]) -> String {
    let bytes: Vec<u8> =
        chars.iter().take_while(|&&byte| byte != 0).map(|&byte| byte as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Forwards shim log output to the `log` crate.
unsafe extern "C" fn shim_log_forwarder(level: i32, msg: *const c_char, _user_data: *mut c_void) {
    if msg.is_null() {
        return;
    }
    // SAFETY: The shim passes a NUL-terminated string valid for the call.
    let msg = unsafe { CStr::from_ptr(msg) }.to_string_lossy();
    let level = match level {
        sys::LK_ARGUS_LOG_ERROR => log::Level::Error,
        sys::LK_ARGUS_LOG_WARN => log::Level::Warn,
        sys::LK_ARGUS_LOG_INFO => log::Level::Info,
        _ => log::Level::Debug,
    };
    log::log!(target: "livekit_capture::argus_shim", level, "{msg}");
}

/// Routes the shim's native log output through the `log` crate, once per
/// process.
fn install_shim_logger() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        // SAFETY: The forwarder is a valid callback for the process lifetime.
        unsafe { sys::lk_argus_set_logger(Some(shim_log_forwarder), std::ptr::null_mut()) };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(index: i32, width: u32, height: u32, min_dur_ns: u64, max_dur_ns: u64) -> SensorMode {
        SensorMode {
            index,
            resolution: VideoResolution::new(width, height),
            min_frame_duration_ns: min_dur_ns,
            max_frame_duration_ns: max_dur_ns,
        }
    }

    /// IMX219-style mode table: 3280x2464@21, 1920x1080@30, 1280x720@60.
    fn imx219_modes() -> Vec<SensorMode> {
        vec![
            mode(0, 3280, 2464, 47_619_048, 500_000_000),
            mode(1, 1920, 1080, 33_333_334, 500_000_000),
            mode(2, 1280, 720, 16_666_667, 500_000_000),
        ]
    }

    fn format(width: u32, height: u32, fps: u32) -> DeviceFormat {
        DeviceFormat::new(VideoResolution::new(width, height), fps, DeviceFrameFormat::Nv12)
    }

    #[test]
    fn only_nv12_is_supported() {
        assert!(is_supported_source_format(DeviceFrameFormat::Nv12));
        assert!(!is_supported_source_format(DeviceFrameFormat::I420));
        assert!(!is_supported_source_format(DeviceFrameFormat::Mjpeg));
    }

    #[test]
    fn frame_duration_tolerance_accepts_rounded_sensor_durations() {
        // 33333334 ns (as sensors report for 30 fps) vs the ideal 33333333.
        let mode = mode(0, 1920, 1080, 33_333_334, 500_000_000);
        assert!(mode.supports_framerate(30));
        assert_eq!(mode.max_framerate_fps(), 30);
    }

    #[test]
    fn negotiates_exact_request_on_smallest_covering_mode() {
        let (format_out, mode) =
            negotiate_format(&DeviceFormatRequest::Exact(format(1280, 720, 30)), &imx219_modes())
                .unwrap();
        // ISP delivers the requested format from the smallest covering mode.
        assert_eq!(format_out, format(1280, 720, 30));
        assert_eq!(mode.index, 2);
    }

    #[test]
    fn exact_request_scales_below_a_larger_mode() {
        let (format_out, mode) =
            negotiate_format(&DeviceFormatRequest::Exact(format(1600, 900, 30)), &imx219_modes())
                .unwrap();
        assert_eq!(format_out, format(1600, 900, 30));
        assert_eq!(mode.index, 1);
    }

    #[test]
    fn exact_request_fails_when_no_mode_covers_it() {
        let result =
            negotiate_format(&DeviceFormatRequest::Exact(format(4000, 3000, 30)), &imx219_modes());
        assert!(matches!(result, Err(DeviceVideoSourceError::UnsupportedFormat(_))));

        let result =
            negotiate_format(&DeviceFormatRequest::Exact(format(3280, 2464, 60)), &imx219_modes());
        assert!(matches!(result, Err(DeviceVideoSourceError::UnsupportedFormat(_))));
    }

    #[test]
    fn closest_request_clamps_resolution_and_framerate() {
        let (format_out, mode) = negotiate_format(
            &DeviceFormatRequest::Closest(format(4000, 3000, 60)),
            &imx219_modes(),
        )
        .unwrap();
        assert_eq!(format_out.resolution, VideoResolution::new(3280, 2464));
        assert_eq!(format_out.framerate_fps, 21);
        assert_eq!(mode.index, 0);
    }

    #[test]
    fn closest_request_passes_through_when_covered() {
        let (format_out, mode) = negotiate_format(
            &DeviceFormatRequest::Closest(format(1920, 1080, 30)),
            &imx219_modes(),
        )
        .unwrap();
        assert_eq!(format_out, format(1920, 1080, 30));
        assert_eq!(mode.index, 1);
    }

    #[test]
    fn default_request_prefers_720p30() {
        let (format_out, mode) =
            negotiate_format(&DeviceFormatRequest::Default, &imx219_modes()).unwrap();
        assert_eq!(format_out, format(1280, 720, 30));
        assert_eq!(mode.index, 2);
    }

    #[test]
    fn default_request_falls_back_to_largest_mode() {
        let modes = vec![mode(0, 640, 480, 33_333_334, 500_000_000)];
        let (format_out, mode_out) =
            negotiate_format(&DeviceFormatRequest::Default, &modes).unwrap();
        assert_eq!(format_out, format(640, 480, 30));
        assert_eq!(mode_out.index, 0);
    }

    #[test]
    fn highest_framerate_selects_fastest_covering_mode() {
        let (format_out, mode) = negotiate_format(
            &DeviceFormatRequest::HighestFramerate { resolution: None, frame_format: None },
            &imx219_modes(),
        )
        .unwrap();
        assert_eq!(format_out.framerate_fps, 60);
        assert_eq!(mode.index, 2);

        let (format_out, mode) = negotiate_format(
            &DeviceFormatRequest::HighestFramerate {
                resolution: Some(VideoResolution::new(1920, 1080)),
                frame_format: None,
            },
            &imx219_modes(),
        )
        .unwrap();
        assert_eq!(format_out, format(1920, 1080, 30));
        assert_eq!(mode.index, 1);
    }

    #[test]
    fn highest_resolution_respects_framerate_constraint() {
        let (format_out, mode) = negotiate_format(
            &DeviceFormatRequest::HighestResolution { framerate_fps: None, frame_format: None },
            &imx219_modes(),
        )
        .unwrap();
        assert_eq!(format_out.resolution, VideoResolution::new(3280, 2464));
        assert_eq!(mode.index, 0);

        let (format_out, mode) = negotiate_format(
            &DeviceFormatRequest::HighestResolution {
                framerate_fps: Some(60),
                frame_format: None,
            },
            &imx219_modes(),
        )
        .unwrap();
        assert_eq!(format_out, format(1280, 720, 60));
        assert_eq!(mode.index, 2);
    }

    #[test]
    fn negotiation_fails_without_modes() {
        let result = negotiate_format(&DeviceFormatRequest::Default, &[]);
        assert!(matches!(result, Err(DeviceVideoSourceError::Backend(_))));
    }

    #[test]
    fn shim_errors_map_to_backend_errors() {
        assert!(matches!(
            map_shim_error(sys::LK_ARGUS_ERR_UNAVAILABLE, "test"),
            DeviceVideoSourceError::DeviceNotFound
        ));
        assert!(matches!(
            map_shim_error(sys::LK_ARGUS_ERR_NO_DEVICE, "test"),
            DeviceVideoSourceError::DeviceNotFound
        ));
        assert!(matches!(
            map_shim_error(sys::LK_ARGUS_ERR_TIMEOUT, "test"),
            DeviceVideoSourceError::FrameTimeout
        ));
        assert!(matches!(
            map_shim_error(sys::LK_ARGUS_ERR_DISCONNECTED, "test"),
            DeviceVideoSourceError::Backend(_)
        ));
    }

    #[test]
    fn mode_formats_are_deduplicated_and_include_common_framerates() {
        let formats = mode_formats(&imx219_modes());
        assert!(formats.contains(&format(1280, 720, 60)));
        assert!(formats.contains(&format(1280, 720, 30)));
        assert!(formats.contains(&format(1280, 720, 15)));
        assert!(formats.contains(&format(1920, 1080, 30)));
        assert!(formats.contains(&format(3280, 2464, 21)));
        assert!(!formats.contains(&format(1920, 1080, 60)));
        let mut deduped = formats.clone();
        deduped.dedup();
        assert_eq!(formats.len(), deduped.len());
    }

    #[test]
    fn c_chars_convert_until_nul() {
        let mut chars = [0 as c_char; 8];
        for (i, byte) in b"abc".iter().enumerate() {
            chars[i] = *byte as c_char;
        }
        assert_eq!(c_chars_to_string(&chars), "abc");
        assert_eq!(c_chars_to_string(&[0 as c_char; 4]), "");
    }
}
