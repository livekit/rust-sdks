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

//! Raw FFI bindings to the LiveKit NVIDIA libargus capture shim.
//!
//! The native shim (`lk_argus.cpp`) wraps NVIDIA's Argus/libargus API for
//! capturing frames from Jetson MIPI CSI cameras as NV12 DMA buffers. It is
//! only compiled and linked on aarch64 Linux targets where the Jetson
//! Multimedia API headers *and* Tegra userspace libraries are found at build
//! time (probed at `/usr/src/jetson_multimedia_api` and
//! `/usr/lib/aarch64-linux-gnu/tegra`; override with the
//! `JETSON_MULTIMEDIA_API_DIR` and `JETSON_TEGRA_LIB_DIR` environment
//! variables).
//!
//! On every other target the crate still builds and exposes the same API,
//! but every entry point is an inert stub returning
//! [`LK_ARGUS_ERR_UNAVAILABLE`]. [`AVAILABLE`] reports which variant was
//! built. This keeps consumers free of build scripts and custom `cfg`s:
//! "not a Jetson" degrades exactly like "Jetson with zero cameras".
//!
//! The C header `src/lk_argus.h` is the source of truth for the ABI; the
//! declarations here mirror it and must be kept in sync.
//!
//! # Thread safety
//!
//! - Enumeration functions, [`lk_argus_set_logger`], [`lk_argus_session_create`],
//!   and [`lk_argus_session_destroy`] may be called from any thread.
//! - Per session, [`lk_argus_frame_acquire`] and
//!   [`lk_argus_frame_copy_to_i420`] must be driven by a single consumer
//!   thread at a time.
//! - [`lk_argus_frame_release`] and [`lk_argus_session_interrupt`] may be
//!   called from any thread.

use std::ffi::{c_char, c_void};

/// Whether the native libargus shim was compiled and linked into this build.
///
/// When `false`, every function in this crate is an inert stub: session and
/// enumeration calls return [`LK_ARGUS_ERR_UNAVAILABLE`].
pub const AVAILABLE: bool = cfg!(libargus_available);

/// ABI version of the shim. Incremented on breaking changes.
pub const LK_ARGUS_ABI_VERSION: u32 = 1;

/// Maximum supported DMA buffer ring size.
pub const LK_ARGUS_MAX_DMA_BUFS: i32 = 16;

pub const LK_ARGUS_OK: i32 = 0;
/// Invalid arguments crossed the FFI boundary (caller bug).
pub const LK_ARGUS_ERR_INVALID_ARG: i32 = -1;
/// CameraProvider creation failed (nvargus-daemon not running?).
pub const LK_ARGUS_ERR_NO_PROVIDER: i32 = -2;
/// Device index out of range.
pub const LK_ARGUS_ERR_NO_DEVICE: i32 = -3;
/// Generic Argus failure; see [`lk_argus_session_last_argus_status`].
pub const LK_ARGUS_ERR_ARGUS: i32 = -4;
/// Frame acquire timed out. Not fatal: retry.
pub const LK_ARGUS_ERR_TIMEOUT: i32 = -5;
/// EGLStream disconnected (nvargus-daemon died or stream ended). The session
/// is dead and must be destroyed and re-created.
pub const LK_ARGUS_ERR_DISCONNECTED: i32 = -6;
/// NvBufSurface operation failed.
pub const LK_ARGUS_ERR_NVBUF: i32 = -7;
/// Every ring slot is leased to an in-flight frame. Not fatal: backpressure;
/// retry after frames are released.
pub const LK_ARGUS_ERR_NO_FREE_BUFFER: i32 = -8;
/// [`lk_argus_session_interrupt`] was called.
pub const LK_ARGUS_ERR_INTERRUPTED: i32 = -9;
/// The native shim was not compiled into this build (see [`AVAILABLE`]).
pub const LK_ARGUS_ERR_UNAVAILABLE: i32 = -10;

/// Log levels passed to [`LkArgusLogFn`].
pub const LK_ARGUS_LOG_ERROR: i32 = 0;
pub const LK_ARGUS_LOG_WARN: i32 = 1;
pub const LK_ARGUS_LOG_INFO: i32 = 2;
pub const LK_ARGUS_LOG_DEBUG: i32 = 3;

/// Returns a human-readable description of a status code.
pub fn lk_argus_status_string(status: i32) -> &'static str {
    match status {
        LK_ARGUS_OK => "ok",
        LK_ARGUS_ERR_INVALID_ARG => "invalid argument",
        LK_ARGUS_ERR_NO_PROVIDER => {
            "failed to create camera provider (is nvargus-daemon running?)"
        }
        LK_ARGUS_ERR_NO_DEVICE => "camera device index out of range",
        LK_ARGUS_ERR_ARGUS => "Argus operation failed",
        LK_ARGUS_ERR_TIMEOUT => "frame acquire timed out",
        LK_ARGUS_ERR_DISCONNECTED => "EGL stream disconnected",
        LK_ARGUS_ERR_NVBUF => "NvBufSurface operation failed",
        LK_ARGUS_ERR_NO_FREE_BUFFER => "all DMA buffers are in flight",
        LK_ARGUS_ERR_INTERRUPTED => "session interrupted",
        LK_ARGUS_ERR_UNAVAILABLE => "libargus shim not available in this build",
        _ => "unknown status",
    }
}

/// Opaque capture session handle.
#[repr(C)]
pub struct LkArgusSession {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LkArgusDeviceInfo {
    /// Camera device UUID, formatted and NUL-terminated.
    pub uuid: [c_char; 37],
    /// Best-effort human-readable module name; empty string when the JetPack
    /// version exposes none.
    pub name: [c_char; 64],
    pub sensor_mode_count: i32,
}

impl Default for LkArgusDeviceInfo {
    fn default() -> Self {
        Self { uuid: [0; 37], name: [0; 64], sensor_mode_count: 0 }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LkArgusSensorModeInfo {
    pub width: u32,
    pub height: u32,
    pub min_frame_duration_ns: u64,
    pub max_frame_duration_ns: u64,
    pub bit_depth: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LkArgusSessionConfig {
    pub device_index: i32,
    /// Sensor mode to use, or -1 to auto-select the smallest mode covering
    /// the requested resolution and frame rate.
    pub sensor_mode_index: i32,
    /// Output (ISP-scaled) resolution and frame rate.
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    /// DMA buffer ring depth; 0 selects the default (4). Clamped to
    /// [2, [`LK_ARGUS_MAX_DMA_BUFS`]].
    pub num_dma_bufs: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct LkArgusFrame {
    /// NV12 DMA buffer fd, *borrowed* from the session ring. Valid until
    /// [`lk_argus_frame_release`] is called with `buffer_index`; never close
    /// it.
    pub dmabuf_fd: i32,
    /// Ring slot index; token for [`lk_argus_frame_release`].
    pub buffer_index: i32,
    pub width: u32,
    pub height: u32,
    /// Actual plane pitches/offsets, Y then interleaved UV.
    pub pitch: [u32; 2],
    pub offset: [u32; 2],
    /// Argus sensor timestamp (`CLOCK_MONOTONIC` domain), 0 when unavailable.
    pub sensor_timestamp_ns: u64,
    /// Diagnostics: time spent waiting in `acquireFrame` and blitting.
    pub acquire_wait_ns: u64,
    pub blit_ns: u64,
}

/// Log callback type for [`lk_argus_set_logger`]. `msg` is only valid for the
/// duration of the call.
pub type LkArgusLogFn =
    unsafe extern "C" fn(level: i32, msg: *const c_char, user_data: *mut c_void);

#[cfg(libargus_available)]
mod ffi {
    use super::*;

    extern "C" {
        pub fn lk_argus_set_logger(log_fn: Option<LkArgusLogFn>, user_data: *mut c_void) -> i32;
        pub fn lk_argus_version(buf: *mut c_char, buf_len: usize) -> i32;
        pub fn lk_argus_device_count() -> i32;
        pub fn lk_argus_device_info(device_index: i32, out: *mut LkArgusDeviceInfo) -> i32;
        pub fn lk_argus_sensor_mode_info(
            device_index: i32,
            mode_index: i32,
            out: *mut LkArgusSensorModeInfo,
        ) -> i32;
        pub fn lk_argus_session_create(
            config: *const LkArgusSessionConfig,
            out_session: *mut *mut LkArgusSession,
        ) -> i32;
        pub fn lk_argus_session_destroy(session: *mut LkArgusSession);
        pub fn lk_argus_session_interrupt(session: *mut LkArgusSession) -> i32;
        pub fn lk_argus_session_last_argus_status(session: *const LkArgusSession) -> i32;
        pub fn lk_argus_frame_acquire(
            session: *mut LkArgusSession,
            timeout_ns: u64,
            out: *mut LkArgusFrame,
        ) -> i32;
        pub fn lk_argus_frame_release(session: *mut LkArgusSession, buffer_index: i32) -> i32;
        pub fn lk_argus_frame_copy_to_i420(
            session: *mut LkArgusSession,
            buffer_index: i32,
            dst_y: *mut u8,
            dst_stride_y: i32,
            dst_u: *mut u8,
            dst_stride_u: i32,
            dst_v: *mut u8,
            dst_stride_v: i32,
        ) -> i32;
    }
}

macro_rules! shim_fns {
    ($(
        $(#[$doc:meta])*
        fn $name:ident($($arg:ident: $ty:ty),* $(,)?) $(-> $ret:ty)? = $stub:expr;
    )*) => {
        $(
            $(#[$doc])*
            ///
            /// # Safety
            /// Pointer arguments must be valid per the contract in
            /// `lk_argus.h`, and the caller must respect the crate-level
            /// thread-safety rules.
            #[inline]
            #[cfg(libargus_available)]
            pub unsafe fn $name($($arg: $ty),*) $(-> $ret)? {
                ffi::$name($($arg),*)
            }

            $(#[$doc])*
            ///
            /// # Safety
            /// Pointer arguments must be valid per the contract in
            /// `lk_argus.h`, and the caller must respect the crate-level
            /// thread-safety rules.
            #[inline]
            #[cfg(not(libargus_available))]
            #[allow(unused_variables)]
            pub unsafe fn $name($($arg: $ty),*) $(-> $ret)? {
                $stub
            }
        )*
    };
}

shim_fns! {
    /// Installs a log callback, replacing stderr output. Pass `None` to
    /// restore stderr logging.
    fn lk_argus_set_logger(log_fn: Option<LkArgusLogFn>, user_data: *mut c_void) -> i32
        = LK_ARGUS_ERR_UNAVAILABLE;

    /// Copies the Argus version string into `buf`, NUL-terminated and
    /// truncated to `buf_len`.
    fn lk_argus_version(buf: *mut c_char, buf_len: usize) -> i32
        = LK_ARGUS_ERR_UNAVAILABLE;

    /// Returns the number of camera devices (>= 0), or a negative status.
    fn lk_argus_device_count() -> i32
        = LK_ARGUS_ERR_UNAVAILABLE;

    /// Fills `out` with information about a camera device.
    fn lk_argus_device_info(device_index: i32, out: *mut LkArgusDeviceInfo) -> i32
        = LK_ARGUS_ERR_UNAVAILABLE;

    /// Fills `out` with information about one of a device's sensor modes.
    fn lk_argus_sensor_mode_info(
        device_index: i32,
        mode_index: i32,
        out: *mut LkArgusSensorModeInfo,
    ) -> i32
        = LK_ARGUS_ERR_UNAVAILABLE;

    /// Opens a capture session. On success writes the session handle to
    /// `out_session`.
    fn lk_argus_session_create(
        config: *const LkArgusSessionConfig,
        out_session: *mut *mut LkArgusSession,
    ) -> i32
        = LK_ARGUS_ERR_UNAVAILABLE;

    /// Tears down a session. Interrupts any pending acquire, stops the
    /// repeating capture, and destroys the DMA buffer ring. All frames must
    /// be released before calling this.
    fn lk_argus_session_destroy(session: *mut LkArgusSession)
        = ();

    /// Makes the pending (and every subsequent) [`lk_argus_frame_acquire`]
    /// return [`LK_ARGUS_ERR_INTERRUPTED`]. Interruption latency is bounded
    /// by the acquire timeout.
    fn lk_argus_session_interrupt(session: *mut LkArgusSession) -> i32
        = LK_ARGUS_ERR_UNAVAILABLE;

    /// Raw `Argus::Status` of the session's last failing Argus call.
    fn lk_argus_session_last_argus_status(session: *const LkArgusSession) -> i32
        = 0;

    /// Acquires the next frame, blocking at most `timeout_ns`. On success
    /// blits the frame into a free ring slot, marks that slot leased, and
    /// fills `out`. The lease (and the fd's validity) lasts until
    /// [`lk_argus_frame_release`] is called with `out.buffer_index`.
    fn lk_argus_frame_acquire(
        session: *mut LkArgusSession,
        timeout_ns: u64,
        out: *mut LkArgusFrame,
    ) -> i32
        = LK_ARGUS_ERR_UNAVAILABLE;

    /// Returns a leased ring slot for reuse. Callable from any thread.
    fn lk_argus_frame_release(session: *mut LkArgusSession, buffer_index: i32) -> i32
        = LK_ARGUS_ERR_UNAVAILABLE;

    /// CPU fallback: copies the NV12 contents of a leased ring slot into
    /// caller-owned I420 planes.
    fn lk_argus_frame_copy_to_i420(
        session: *mut LkArgusSession,
        buffer_index: i32,
        dst_y: *mut u8,
        dst_stride_y: i32,
        dst_u: *mut u8,
        dst_stride_u: i32,
        dst_v: *mut u8,
        dst_stride_v: i32,
    ) -> i32
        = LK_ARGUS_ERR_UNAVAILABLE;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_strings_cover_all_codes() {
        for code in [
            LK_ARGUS_OK,
            LK_ARGUS_ERR_INVALID_ARG,
            LK_ARGUS_ERR_NO_PROVIDER,
            LK_ARGUS_ERR_NO_DEVICE,
            LK_ARGUS_ERR_ARGUS,
            LK_ARGUS_ERR_TIMEOUT,
            LK_ARGUS_ERR_DISCONNECTED,
            LK_ARGUS_ERR_NVBUF,
            LK_ARGUS_ERR_NO_FREE_BUFFER,
            LK_ARGUS_ERR_INTERRUPTED,
            LK_ARGUS_ERR_UNAVAILABLE,
        ] {
            assert_ne!(lk_argus_status_string(code), "unknown status");
        }
        assert_eq!(lk_argus_status_string(-999), "unknown status");
    }

    #[cfg(not(libargus_available))]
    #[test]
    fn stubs_report_unavailable() {
        assert!(!AVAILABLE);
        unsafe {
            assert_eq!(lk_argus_device_count(), LK_ARGUS_ERR_UNAVAILABLE);
            let mut out = LkArgusFrame::default();
            assert_eq!(
                lk_argus_frame_acquire(std::ptr::null_mut(), 0, &mut out),
                LK_ARGUS_ERR_UNAVAILABLE
            );
        }
    }
}
