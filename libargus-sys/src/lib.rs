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
//! capturing frames from Jetson MIPI CSI cameras as NV12 DMA-BUFs. It is only
//! compiled and linked on `aarch64` Linux (Jetson) targets where the Jetson
//! Multimedia API headers are available at build time; on every other target
//! the crate builds successfully but exposes no bindings.
//!
//! Use [`AVAILABLE`] to check whether the native shim was linked into the
//! current build before referencing any of the `extern "C"` functions, which
//! are only present when [`AVAILABLE`] is `true`.

/// Whether the native libargus shim was compiled and linked into this build.
///
/// This is `true` only on Jetson (`aarch64` Linux) targets where the Jetson
/// Multimedia API headers were found at build time. When `false`, none of the
/// `lk_argus_*` bindings are available.
pub const AVAILABLE: bool = cfg!(libargus_available);

#[cfg(libargus_available)]
mod ffi {
    use std::ffi::{c_int, c_void};

    extern "C" {
        /// Opens an Argus capture session for `sensor_index` at the requested
        /// resolution and frame rate. Returns an opaque session pointer, or
        /// null on failure.
        pub fn lk_argus_create_session(
            sensor_index: c_int,
            width: c_int,
            height: c_int,
            fps: c_int,
        ) -> *mut c_void;

        /// Tears down a session previously returned by
        /// [`lk_argus_create_session`].
        pub fn lk_argus_destroy_session(session: *mut c_void);

        /// Acquires the next captured frame, blitting it into a DMA-BUF from the
        /// session's ring. Returns the DMA-BUF fd on success (negative on
        /// failure) and writes capture metadata to the out-pointers when
        /// non-null.
        pub fn lk_argus_acquire_frame_with_metadata(
            session: *mut c_void,
            sensor_timestamp_ns: *mut u64,
            acquire_wait_ns: *mut u64,
            blit_ns: *mut u64,
        ) -> c_int;

        /// Copies the NV12 DMA-BUF identified by `dmabuf_fd` into caller-owned
        /// I420 planes. Returns 0 on success or a negative status code.
        pub fn lk_argus_copy_frame_to_i420(
            session: *mut c_void,
            dmabuf_fd: c_int,
            dst_y: *mut u8,
            dst_stride_y: c_int,
            dst_u: *mut u8,
            dst_stride_u: c_int,
            dst_v: *mut u8,
            dst_stride_v: c_int,
            copy_to_i420_ns: *mut u64,
        ) -> c_int;

        /// Releases the frame currently held by the session, if any.
        pub fn lk_argus_release_frame(session: *mut c_void);
    }
}

#[cfg(libargus_available)]
pub use ffi::*;
