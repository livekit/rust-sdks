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

//! Runtime loading of the GStreamer source from the
//! `livekit-capture-gstreamer` shim library.
//!
//! The shim is loaded when the first source is created, from the path in
//! [`LIBRARY_ENV`] or by its platform file name through the system library
//! search path. Once loaded, it stays loaded for the life of the process:
//! GStreamer and GLib do not support being unloaded.

use std::{
    ffi::{c_void, CStr, CString},
    ptr::{self, NonNull},
    sync::OnceLock,
};

use bytes::Bytes;
use libloading::Library;
use thiserror::Error;

use super::{abi, GStreamerVideoSourceConfig};
use crate::{
    encoded::{
        EncodedFrameType, EncodedRateControl, EncodedVideoCodec, EncodedVideoSource,
        OwnedEncodedAccessUnit,
    },
    error::SourceError,
    primitive::VideoResolution,
    pump::PumpStop,
};

/// Environment variable that overrides the shim library path.
const LIBRARY_ENV: &str = "LIVEKIT_CAPTURE_GSTREAMER_LIBRARY";

/// Shim library name, without platform prefix or suffix.
const LIBRARY_NAME: &str = "livekit_capture_gstreamer";

static SHIM: OnceLock<Shim> = OnceLock::new();

/// Function table resolved from the shim library.
#[derive(Debug)]
struct Shim {
    open: abi::OpenFn,
    next: abi::NextFn,
    request_keyframe: abi::RequestKeyframeFn,
    update_rate_control: abi::UpdateRateControlFn,
    close: abi::CloseFn,
    // Keeps the function pointers above valid; never dropped once stored in
    // `SHIM`.
    _library: Library,
}

impl Shim {
    /// Returns the process-wide shim, loading it on first use.
    ///
    /// A failed load is not cached, so a later call retries.
    fn get() -> Result<&'static Self, GStreamerVideoSourceError> {
        if let Some(shim) = SHIM.get() {
            return Ok(shim);
        }
        let shim = Self::load()?;
        // A concurrent loser drops its `Library`, which only decrements the
        // OS reference count of the library the winner keeps.
        Ok(SHIM.get_or_init(|| shim))
    }

    fn load() -> Result<Self, GStreamerVideoSourceError> {
        let path = std::env::var_os(LIBRARY_ENV)
            .unwrap_or_else(|| libloading::library_filename(LIBRARY_NAME));
        // SAFETY: the shim's initializers only set up Rust statics; GStreamer
        // itself is initialized by the first `lk_gst_open`.
        let library = unsafe { Library::new(path) }?;

        // SAFETY: `lk_gst_abi_version` has the same signature in every ABI
        // version, so it is safe to call before the version is checked.
        let version = unsafe { library.get::<abi::AbiVersionFn>("lk_gst_abi_version")?() };
        if version != abi::VERSION {
            return Err(GStreamerVideoSourceError::AbiMismatch {
                expected: abi::VERSION,
                found: version,
            });
        }

        // SAFETY: the version check above guarantees the exported symbols
        // have the signatures declared in `abi`. The copied function pointers
        // stay valid because `library` is stored alongside them.
        unsafe {
            Ok(Self {
                open: *library.get("lk_gst_open")?,
                next: *library.get("lk_gst_next")?,
                request_keyframe: *library.get("lk_gst_request_keyframe")?,
                update_rate_control: *library.get("lk_gst_update_rate_control")?,
                close: *library.get("lk_gst_close")?,
                _library: library,
            })
        }
    }
}

/// Encoded source backed by a GStreamer pipeline running in the shim library.
#[derive(Debug)]
pub struct GStreamerVideoSource {
    shim: &'static Shim,
    handle: NonNull<c_void>,
    codec: EncodedVideoCodec,
    resolution: VideoResolution,
}

// SAFETY: the handle owns the shim's `GStreamerVideoSource`, which is `Send`,
// and is only used through `&mut self` or on drop.
unsafe impl Send for GStreamerVideoSource {}

impl GStreamerVideoSource {
    /// Creates the source. Construction and stream discovery run on the
    /// tokio blocking pool.
    ///
    /// Requires a running tokio runtime. Use
    /// [`GStreamerVideoSource::new_blocking`] outside of async contexts.
    pub async fn new(config: GStreamerVideoSourceConfig) -> Result<Self, SourceError> {
        match tokio::task::spawn_blocking(move || Self::new_blocking(config)).await {
            Ok(result) => result,
            Err(err) if err.is_panic() => std::panic::resume_unwind(err.into_panic()),
            Err(err) => Err(SourceError::new(err)),
        }
    }

    /// Loads the shim library if needed, then builds and starts the
    /// GStreamer pipeline from the configuration.
    ///
    /// Fails if the shim cannot be loaded, in addition to the failures of
    /// the linked source.
    pub fn new_blocking(config: GStreamerVideoSourceConfig) -> Result<Self, SourceError> {
        let shim = Shim::get().map_err(SourceError::new)?;
        let config = serde_json::to_string(&config).map_err(SourceError::new)?;
        let config = CString::new(config).map_err(SourceError::new)?;

        let mut info = abi::StreamInfo::default();
        let mut error = [0u8; abi::ERROR_LEN];
        // SAFETY: all pointers are valid for the duration of the call, and
        // `error` holds `error.len()` writable bytes.
        let handle = unsafe {
            (shim.open)(config.as_ptr(), &mut info, error.as_mut_ptr().cast(), error.len())
        };
        let Some(handle) = NonNull::new(handle) else {
            return Err(SourceError::new(shim_error(&error)));
        };
        let Some(codec) = abi::codec_from_raw(info.codec) else {
            // SAFETY: `handle` came from `open` and is not used afterwards.
            unsafe { (shim.close)(handle.as_ptr()) };
            return Err(SourceError::new(GStreamerVideoSourceError::Shim(format!(
                "unknown codec {}",
                info.codec
            ))));
        };

        Ok(Self { shim, handle, codec, resolution: VideoResolution::new(info.width, info.height) })
    }
}

impl Drop for GStreamerVideoSource {
    fn drop(&mut self) {
        // SAFETY: `handle` came from `open` and is not used after this.
        unsafe { (self.shim.close)(self.handle.as_ptr()) };
    }
}

impl EncodedVideoSource for GStreamerVideoSource {
    fn resolution(&self) -> VideoResolution {
        self.resolution
    }

    fn codec(&self) -> EncodedVideoCodec {
        self.codec
    }

    fn next_access_unit(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<OwnedEncodedAccessUnit>, SourceError> {
        // Each call into the shim waits a bounded time, so the stop token is
        // observed promptly even while the pipeline produces nothing.
        loop {
            if stop.is_stopped() {
                return Ok(None);
            }
            let mut unit =
                abi::AccessUnit { data: ptr::null(), len: 0, timestamp_us: 0, keyframe: false };
            let mut error = [0u8; abi::ERROR_LEN];
            // SAFETY: `handle` is live, all pointers are valid for the
            // duration of the call, and `error` holds `error.len()` bytes.
            let status = unsafe {
                (self.shim.next)(
                    self.handle.as_ptr(),
                    &mut unit,
                    error.as_mut_ptr().cast(),
                    error.len(),
                )
            };
            match status {
                abi::NEXT_UNIT => {
                    // SAFETY: the shim keeps `data` valid for `len` bytes until
                    // the next call on this handle; the bytes are copied here.
                    let payload = unsafe { std::slice::from_raw_parts(unit.data, unit.len) };
                    let frame_type =
                        if unit.keyframe { EncodedFrameType::Key } else { EncodedFrameType::Delta };
                    return Ok(Some(OwnedEncodedAccessUnit::new(
                        self.codec,
                        Bytes::copy_from_slice(payload),
                        unit.timestamp_us,
                        frame_type,
                        self.resolution,
                    )));
                }
                abi::NEXT_PENDING => {}
                abi::NEXT_END => return Ok(None),
                _ => return Err(SourceError::new(shim_error(&error))),
            }
        }
    }

    fn request_keyframe(&mut self) {
        // SAFETY: `handle` is live.
        unsafe { (self.shim.request_keyframe)(self.handle.as_ptr()) };
    }

    fn update_rate_control(&mut self, rate_control: EncodedRateControl) {
        // SAFETY: `handle` is live.
        unsafe {
            (self.shim.update_rate_control)(
                self.handle.as_ptr(),
                rate_control.target_bitrate_bps,
                rate_control.framerate_fps,
            )
        };
    }
}

/// Reads the message the shim wrote into an error buffer.
fn shim_error(error: &[u8]) -> GStreamerVideoSourceError {
    let message = CStr::from_bytes_until_nul(error)
        .map(|message| message.to_string_lossy().into_owned())
        .unwrap_or_default();
    GStreamerVideoSourceError::Shim(message)
}

/// Error returned by the runtime-loaded GStreamer source.
#[derive(Debug, Error)]
pub enum GStreamerVideoSourceError {
    /// The shim library could not be loaded or lacks a required symbol.
    #[error("failed to load the GStreamer shim library (set {LIBRARY_ENV} to its path): {0}")]
    Load(#[from] libloading::Error),
    /// The shim library was built for a different ABI version.
    #[error("GStreamer shim library has ABI version {found}, expected {expected}")]
    AbiMismatch {
        /// Version this crate expects.
        expected: u32,
        /// Version the shim reports.
        found: u32,
    },
    /// The shim reported an error from the GStreamer source.
    #[error("{0}")]
    Shim(String),
}
