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

use std::ffi::{c_char, c_ulong, c_void};
use thiserror::Error;

/// Settings for the audio sampler.
#[derive(Debug)]
pub struct ResamplerSettings {
    /// The sample rate of the input audio data (in Hz).
    pub input_rate: f64,

    /// The desired sample rate of the output audio data (in Hz).
    pub output_rate: f64,

    /// The number of audio channels (e.g., 1 for mono, 2 for stereo).
    pub num_channels: u32,

    /// The quality setting for the resampler.
    pub quality: ResamplerQuality,
}

/// Quality setting for the audio resampler.
///
/// Higher quality settings result in better audio quality but
/// require more processing power.
///
#[derive(Debug)]
#[repr(u32)]
pub enum ResamplerQuality {
    Quick = 0,
    Low,
    Medium,
    High,
    VeryHigh,
}

/// Audio processor for one-dimensional sample-rate conversion.
#[derive(Debug)]
pub struct Resampler {
    soxr_ptr: soxr_sys::soxr_t,
    out_buf: Vec<i16>,
    input_rate: f64,
    output_rate: f64,
    num_channels: u32,
}

/// An error that can occur during audio resampler initialization or processing.
#[derive(Debug, Error)]
pub enum ResamplerError {
    /// Resampler could not be initialized.
    #[error("Resampler could not be initialized: {0}")]
    Initialization(String),

    /// Resampler operation failed.
    #[error("Resampling operation failed: {0}")]
    OperationFailed(String),
}

impl Resampler {
    /// Creates a new audio resampler with the given settings.
    pub fn new(settings: ResamplerSettings) -> Result<Resampler, ResamplerError> {
        let error: *mut *const c_char = std::ptr::null_mut();

        let soxr_ptr = unsafe {
            // TODO: for now we just support interleaved; add support for planar if needed.
            let io_spec = soxr_sys::soxr_io_spec(
                soxr_sys::soxr_datatype_t_SOXR_INT16_I, // Input
                soxr_sys::soxr_datatype_t_SOXR_INT16_I, // Output
            );

            let quality_spec = soxr_sys::soxr_quality_spec(
                settings.quality as c_ulong,
                0 as c_ulong, // TODO: expose flag options
            );

            // TODO: allow changing thread count.
            let runtime_spec = soxr_sys::soxr_runtime_spec(1);

            soxr_sys::soxr_create(
                settings.input_rate,
                settings.output_rate,
                settings.num_channels,
                error,
                &io_spec,
                &quality_spec,
                &runtime_spec,
            )
        };

        if !error.is_null() {
            let error_msg =
                unsafe { std::ffi::CStr::from_ptr(*error) }.to_string_lossy().to_string();
            Err(ResamplerError::Initialization(error_msg))?
        }
        let out_buf = Vec::with_capacity(settings.output_rate as usize / 100);
        Ok(Self {
            soxr_ptr,
            out_buf,
            input_rate: settings.input_rate,
            output_rate: settings.output_rate,
            num_channels: settings.num_channels,
        })
    }

    /// Push audio data into the resampler and retrieve any available resampled data.
    ///
    /// This method accepts audio data, resamples it according to the configured input
    /// and output rates, and returns any resampled data that is available after processing the input.
    ///
    pub fn push(&mut self, input: &[i16]) -> Result<&[i16], ResamplerError> {
        let input_length = input.len() / self.num_channels as usize;
        let ratio = self.output_rate / self.input_rate;
        let soxr_delay = unsafe { soxr_sys::soxr_delay(self.soxr_ptr) };

        let max_out_len =
            ((input_length as f64 * ratio).ceil() as usize) + (soxr_delay.ceil() as usize) + 1;

        let required_output_size = max_out_len * self.num_channels as usize;
        if self.out_buf.len() < required_output_size {
            self.out_buf.resize(required_output_size, 0);
        }

        let mut idone: usize = 0;
        let mut odone: usize = 0;
        let error = unsafe {
            soxr_sys::soxr_process(
                self.soxr_ptr,
                input.as_ptr() as *const c_void,
                input_length,
                &mut idone,
                self.out_buf.as_mut_ptr() as *mut c_void,
                max_out_len,
                &mut odone,
            )
        };
        if !error.is_null() {
            let error_msg =
                unsafe { std::ffi::CStr::from_ptr(error) }.to_string_lossy().to_string();
            Err(ResamplerError::OperationFailed(error_msg))?
        }

        let output_samples = odone * self.num_channels as usize;
        Ok(&self.out_buf[..output_samples])
    }

    /// Flush any remaining audio data through the resampler and retrieve the resampled data.
    ///
    /// This method should be called when no more input data will be provided to ensure that all
    /// internal buffers are processed and all resampled data is output.
    ///
    pub fn flush(&mut self) -> Result<&[i16], ResamplerError> {
        let mut odone: usize = 0;
        let error = unsafe {
            soxr_sys::soxr_process(
                self.soxr_ptr,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                self.out_buf.as_mut_ptr() as *mut c_void,
                self.out_buf.len(),
                &mut odone,
            )
        };
        if !error.is_null() {
            let error_msg =
                unsafe { std::ffi::CStr::from_ptr(error) }.to_string_lossy().to_string();
            Err(ResamplerError::OperationFailed(error_msg))?
        }

        let error = unsafe { soxr_sys::soxr_clear(self.soxr_ptr) };

        if !error.is_null() {
            let error_msg =
                unsafe { std::ffi::CStr::from_ptr(error) }.to_string_lossy().to_string();
            Err(ResamplerError::OperationFailed(error_msg))?
        }

        let output_samples = odone * self.num_channels as usize;
        Ok(&self.out_buf[..output_samples])
    }
}

unsafe impl Send for Resampler {}

impl Drop for Resampler {
    fn drop(&mut self) {
        unsafe {
            soxr_sys::soxr_delete(self.soxr_ptr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample() {
        let settings = ResamplerSettings {
            input_rate: 48_000.0,
            output_rate: 24_000.0,
            num_channels: 2,
            quality: ResamplerQuality::Medium,
        };
        let mut resampler = Resampler::new(settings).expect("Initialization failed");
        resampler.push(&vec![0; 512]).expect("Push failed");
        let flushed_samples = resampler.flush().expect("Flush failed");
        assert_eq!(flushed_samples.len(), 256);
    }
}
