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

//! Rockchip MPP MJPEG decoding for the direct V4L2 publisher path.

use anyhow::Result;
use livekit::webrtc::video_frame::{NV12Buffer, VideoBuffer};

#[cfg(lk_mpp)]
mod imp {
    use super::*;
    use anyhow::{anyhow, ensure};
    use std::ffi::{c_char, CStr};
    use std::ptr::NonNull;

    const ERROR_CAPACITY: usize = 512;

    #[repr(C)]
    struct RawMppMjpegDecoder {
        _private: [u8; 0],
    }

    unsafe extern "C" {
        fn lk_mpp_mjpeg_decoder_create(
            width: u32,
            height: u32,
            error: *mut c_char,
            error_capacity: usize,
        ) -> *mut RawMppMjpegDecoder;

        fn lk_mpp_mjpeg_decoder_decode(
            decoder: *mut RawMppMjpegDecoder,
            source: *const u8,
            source_size: usize,
            destination_y: *mut u8,
            destination_y_size: usize,
            destination_stride_y: u32,
            destination_uv: *mut u8,
            destination_uv_size: usize,
            destination_stride_uv: u32,
            error: *mut c_char,
            error_capacity: usize,
        ) -> i32;

        fn lk_mpp_mjpeg_decoder_destroy(decoder: *mut RawMppMjpegDecoder);
    }

    /// Owns one persistent Rockchip MPP MJPEG decoder context.
    pub(crate) struct MppMjpegDecoder {
        handle: NonNull<RawMppMjpegDecoder>,
        width: u32,
        height: u32,
    }

    impl MppMjpegDecoder {
        /// Creates a hardware MJPEG decoder for a fixed output resolution.
        pub(crate) fn new(width: u32, height: u32) -> Result<Self> {
            let mut error = [0_i8; ERROR_CAPACITY];
            // SAFETY: error is writable for ERROR_CAPACITY bytes. The returned
            // handle, when non-null, is uniquely owned until Drop.
            let handle = unsafe {
                lk_mpp_mjpeg_decoder_create(width, height, error.as_mut_ptr(), error.len())
            };
            let handle = NonNull::new(handle).ok_or_else(|| error_message(&error))?;
            Ok(Self { handle, width, height })
        }

        /// Decodes one complete JPEG frame into an NV12 buffer.
        pub(crate) fn decode(&mut self, source: &[u8], destination: &mut NV12Buffer) -> Result<()> {
            ensure!(
                destination.width() == self.width && destination.height() == self.height,
                "MPP MJPEG destination is {}x{}, expected {}x{}",
                destination.width(),
                destination.height(),
                self.width,
                self.height,
            );
            let (stride_y, stride_uv) = destination.strides();
            let (destination_y, destination_uv) = destination.data_mut();
            let mut error = [0_i8; ERROR_CAPACITY];
            // SAFETY: handle is a live uniquely-owned decoder. All input and
            // output pointers use their exact slice lengths, and the decoder
            // validates dimensions and strides before writing.
            let result = unsafe {
                lk_mpp_mjpeg_decoder_decode(
                    self.handle.as_ptr(),
                    source.as_ptr(),
                    source.len(),
                    destination_y.as_mut_ptr(),
                    destination_y.len(),
                    stride_y,
                    destination_uv.as_mut_ptr(),
                    destination_uv.len(),
                    stride_uv,
                    error.as_mut_ptr(),
                    error.len(),
                )
            };
            ensure!(result == 0, "{}", error_message(&error));
            Ok(())
        }
    }

    impl Drop for MppMjpegDecoder {
        fn drop(&mut self) {
            // SAFETY: handle was returned by create and is released exactly once.
            unsafe { lk_mpp_mjpeg_decoder_destroy(self.handle.as_ptr()) };
        }
    }

    fn error_message(error: &[c_char]) -> anyhow::Error {
        // SAFETY: the C++ boundary always NUL-terminates this fixed-size buffer.
        let message = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy();
        if message.is_empty() {
            anyhow!("Rockchip MPP MJPEG decoder failed without a diagnostic")
        } else {
            anyhow!(message.into_owned())
        }
    }
}

#[cfg(not(lk_mpp))]
mod imp {
    use super::*;
    use anyhow::bail;

    /// Placeholder used when the publisher was built without Rockchip MPP headers.
    pub(crate) struct MppMjpegDecoder;

    impl MppMjpegDecoder {
        /// Reports that this build does not include the native MPP decoder bridge.
        pub(crate) fn new(_width: u32, _height: u32) -> Result<Self> {
            bail!(
                "publisher was built without Rockchip MPP support; install librockchip-mpp-dev and rebuild"
            )
        }

        /// Reports that hardware MJPEG decoding is unavailable in this build.
        pub(crate) fn decode(
            &mut self,
            _source: &[u8],
            _destination: &mut NV12Buffer,
        ) -> Result<()> {
            bail!("Rockchip MPP MJPEG decoder is unavailable")
        }
    }
}

pub(crate) use imp::MppMjpegDecoder;
