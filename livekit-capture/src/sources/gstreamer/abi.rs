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

//! C ABI between the runtime loader and the `livekit-capture-gstreamer`
//! shim library.
//!
//! The shim exports one symbol per function type below, named after the
//! type (`lk_gst_open` for [`OpenFn`], and so on). The configuration crosses
//! as JSON. Bump [`VERSION`] on any change to this module, the exported
//! symbols, or the configuration types.

use std::ffi::{c_char, c_void};

use crate::encoded::EncodedVideoCodec;

/// ABI version returned by `lk_gst_abi_version`.
pub const VERSION: u32 = 1;

/// Size of the error buffers the loader passes to the shim.
pub const ERROR_LEN: usize = 1024;

/// `lk_gst_next` filled in an access unit.
pub const NEXT_UNIT: i32 = 0;
/// `lk_gst_next` waited without producing an access unit.
pub const NEXT_PENDING: i32 = 1;
/// `lk_gst_next` reached the end of the stream.
pub const NEXT_END: i32 = 2;
/// `lk_gst_next` failed and wrote an error message.
pub const NEXT_ERROR: i32 = -1;

/// Stream settings reported by `lk_gst_open`.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct StreamInfo {
    /// Codec, as encoded by [`codec_to_raw`].
    pub codec: u32,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
}

/// Access unit reported by `lk_gst_next`.
///
/// `data` stays valid until the next call on the same source.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AccessUnit {
    /// Payload start.
    pub data: *const u8,
    /// Payload length in bytes.
    pub len: usize,
    /// Capture timestamp in microseconds.
    pub timestamp_us: i64,
    /// Whether the unit is a keyframe.
    pub keyframe: bool,
}

/// `lk_gst_abi_version`: returns [`VERSION`].
pub type AbiVersionFn = unsafe extern "C" fn() -> u32;

/// `lk_gst_open`: creates a source from a NUL-terminated JSON configuration.
/// Returns null and writes an error message on failure.
pub type OpenFn = unsafe extern "C" fn(
    config: *const c_char,
    info: *mut StreamInfo,
    error: *mut c_char,
    error_len: usize,
) -> *mut c_void;

/// `lk_gst_next`: waits a bounded time for the next access unit and returns
/// one of the `NEXT_*` codes.
pub type NextFn = unsafe extern "C" fn(
    source: *mut c_void,
    unit: *mut AccessUnit,
    error: *mut c_char,
    error_len: usize,
) -> i32;

/// `lk_gst_request_keyframe`: forwards a keyframe request.
pub type RequestKeyframeFn = unsafe extern "C" fn(source: *mut c_void);

/// `lk_gst_update_rate_control`: forwards a rate-control target.
pub type UpdateRateControlFn =
    unsafe extern "C" fn(source: *mut c_void, target_bitrate_bps: u64, framerate_fps: f64);

/// `lk_gst_close`: destroys a source created by `lk_gst_open`.
pub type CloseFn = unsafe extern "C" fn(source: *mut c_void);

/// Encodes a codec for [`StreamInfo::codec`].
pub fn codec_to_raw(codec: EncodedVideoCodec) -> u32 {
    match codec {
        EncodedVideoCodec::H264 => 0,
        EncodedVideoCodec::H265 => 1,
        EncodedVideoCodec::VP8 => 2,
        EncodedVideoCodec::VP9 => 3,
        EncodedVideoCodec::AV1 => 4,
    }
}

/// Decodes [`StreamInfo::codec`].
pub fn codec_from_raw(codec: u32) -> Option<EncodedVideoCodec> {
    match codec {
        0 => Some(EncodedVideoCodec::H264),
        1 => Some(EncodedVideoCodec::H265),
        2 => Some(EncodedVideoCodec::VP8),
        3 => Some(EncodedVideoCodec::VP9),
        4 => Some(EncodedVideoCodec::AV1),
        _ => None,
    }
}

/// Writes `message` into an error buffer as a NUL-terminated string,
/// truncating it to fit.
///
/// # Safety
///
/// `error` must be null or valid for writes of `error_len` bytes.
pub unsafe fn write_error(error: *mut c_char, error_len: usize, message: &str) {
    if error.is_null() || error_len == 0 {
        return;
    }
    let len = message.len().min(error_len - 1);
    // SAFETY: the caller guarantees `error_len` writable bytes, and at most
    // `error_len - 1` message bytes plus the terminator are written.
    unsafe {
        std::ptr::copy_nonoverlapping(message.as_ptr(), error.cast::<u8>(), len);
        error.add(len).write(0);
    }
}
