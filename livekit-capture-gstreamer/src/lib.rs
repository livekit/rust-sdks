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

//! Exports `livekit-capture`'s linked [`GStreamerVideoSource`] over the C ABI
//! in [`livekit_capture::sources::gstreamer::abi`], for runtime loading by the
//! `source-gstreamer-dynamic` feature.

use std::{
    ffi::{c_char, c_void, CStr},
    panic::{catch_unwind, AssertUnwindSafe},
    ptr,
    task::Poll,
};

use livekit_capture::{
    encoded::{EncodedFrameType, EncodedRateControl, EncodedVideoSource, OwnedEncodedAccessUnit},
    error::SourceError,
    sources::gstreamer::{abi, GStreamerVideoSource, GStreamerVideoSourceConfig},
};

/// Source behind an `lk_gst_open` handle.
struct Source {
    inner: GStreamerVideoSource,
    // Owns the payload last handed out by `lk_gst_next`.
    unit: Option<OwnedEncodedAccessUnit>,
}

#[no_mangle]
pub extern "C" fn lk_gst_abi_version() -> u32 {
    abi::VERSION
}

/// # Safety
///
/// See [`abi::OpenFn`]. `config` must be NUL-terminated, `info` valid for
/// writes, and `error` valid for writes of `error_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn lk_gst_open(
    config: *const c_char,
    info: *mut abi::StreamInfo,
    error: *mut c_char,
    error_len: usize,
) -> *mut c_void {
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: the caller passes a NUL-terminated string.
        let config = unsafe { CStr::from_ptr(config) }.to_str().map_err(|err| err.to_string())?;
        let config: GStreamerVideoSourceConfig =
            serde_json::from_str(config).map_err(|err| err.to_string())?;
        GStreamerVideoSource::new_blocking(config).map_err(|err| err.to_string())
    }))
    .unwrap_or_else(|_| Err("GStreamer source panicked during construction".to_owned()));

    match result {
        Ok(inner) => {
            let resolution = inner.resolution();
            // SAFETY: the caller passes a pointer valid for writes.
            unsafe {
                info.write(abi::StreamInfo {
                    codec: abi::codec_to_raw(inner.codec()),
                    width: resolution.width,
                    height: resolution.height,
                })
            };
            Box::into_raw(Box::new(Source { inner, unit: None })).cast()
        }
        Err(message) => {
            // SAFETY: the caller passes a buffer of `error_len` bytes.
            unsafe { abi::write_error(error, error_len, &message) };
            ptr::null_mut()
        }
    }
}

/// # Safety
///
/// See [`abi::NextFn`]. `source` must come from [`lk_gst_open`] and not be
/// used concurrently; `unit` must be valid for writes and `error` for writes
/// of `error_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn lk_gst_next(
    source: *mut c_void,
    unit: *mut abi::AccessUnit,
    error: *mut c_char,
    error_len: usize,
) -> i32 {
    // SAFETY: the caller passes a live, exclusively used handle.
    let source = unsafe { &mut *source.cast::<Source>() };
    source.unit = None;

    let result = catch_unwind(AssertUnwindSafe(|| source.inner.poll_access_unit()))
        .unwrap_or_else(|_| Err(SourceError::new("GStreamer source panicked")));
    match result {
        Ok(Poll::Ready(Some(next))) => {
            // SAFETY: the caller passes a pointer valid for writes. The
            // payload stays owned by `source.unit` until the next call.
            unsafe {
                unit.write(abi::AccessUnit {
                    data: next.payload.as_ptr(),
                    len: next.payload.len(),
                    timestamp_us: next.timestamp_us,
                    keyframe: next.frame_type == EncodedFrameType::Key,
                })
            };
            source.unit = Some(next);
            abi::NEXT_UNIT
        }
        Ok(Poll::Ready(None)) => abi::NEXT_END,
        Ok(Poll::Pending) => abi::NEXT_PENDING,
        Err(err) => {
            // SAFETY: the caller passes a buffer of `error_len` bytes.
            unsafe { abi::write_error(error, error_len, &err.to_string()) };
            abi::NEXT_ERROR
        }
    }
}

/// # Safety
///
/// `source` must come from [`lk_gst_open`] and not be used concurrently.
#[no_mangle]
pub unsafe extern "C" fn lk_gst_request_keyframe(source: *mut c_void) {
    // SAFETY: the caller passes a live, exclusively used handle.
    let source = unsafe { &mut *source.cast::<Source>() };
    let _ = catch_unwind(AssertUnwindSafe(|| source.inner.request_keyframe()));
}

/// # Safety
///
/// `source` must come from [`lk_gst_open`] and not be used concurrently.
#[no_mangle]
pub unsafe extern "C" fn lk_gst_update_rate_control(
    source: *mut c_void,
    target_bitrate_bps: u64,
    framerate_fps: f64,
) {
    // SAFETY: the caller passes a live, exclusively used handle.
    let source = unsafe { &mut *source.cast::<Source>() };
    let target = EncodedRateControl { target_bitrate_bps, framerate_fps };
    let _ = catch_unwind(AssertUnwindSafe(|| source.inner.update_rate_control(target)));
}

/// # Safety
///
/// `source` must come from [`lk_gst_open`] and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn lk_gst_close(source: *mut c_void) {
    // SAFETY: the caller passes ownership of a handle from `lk_gst_open`.
    let source = unsafe { Box::from_raw(source.cast::<Source>()) };
    let _ = catch_unwind(AssertUnwindSafe(|| drop(source)));
}
