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

use std::{
    ffi::c_char,
    os::raw::{c_ulong, c_void},
};

use soxr_sys;

use crate::proto;

pub struct IOSpec {
    pub input_type: proto::SoxResamplerDataType,
    pub output_type: proto::SoxResamplerDataType,
}

pub struct QualitySpec {
    pub quality: proto::SoxQualityRecipe,
    pub flags: u32, // proto::SoxQualityFlags
}

pub struct RuntimeSpec {
    pub num_threads: u32,
}

pub struct SoxResampler {
    soxr_ptr: soxr_sys::soxr_t,
    out_buf: Vec<i16>,
    input_rate: f64,
    output_rate: f64,
    num_channels: u32,
}

unsafe impl Send for SoxResampler {}

impl SoxResampler {
    pub fn new(
        input_rate: f64,
        output_rate: f64,
        num_channels: u32,
        io_spec: IOSpec,
        quality_spec: QualitySpec,
        runtime_spec: RuntimeSpec,
    ) -> Result<Self, String> {
        let error: *mut *const c_char = std::ptr::null_mut();

        let soxr_ptr = unsafe {
            let io_spec = soxr_sys::soxr_io_spec(
                to_soxr_datatype(io_spec.input_type),
                to_soxr_datatype(io_spec.output_type),
            );

            let quality_spec = soxr_sys::soxr_quality_spec(
                quality_spec.quality as c_ulong,
                quality_spec.flags as c_ulong,
            );

            let runtime_spec = soxr_sys::soxr_runtime_spec(runtime_spec.num_threads);

            soxr_sys::soxr_create(
                input_rate,
                output_rate,
                num_channels,
                error,
                &io_spec,
                &quality_spec,
                &runtime_spec,
            )
        };

        if !error.is_null() {
            let error_msg = unsafe { std::ffi::CStr::from_ptr(*error) };
            return Err(error_msg.to_string_lossy().to_string());
        }

        Ok(Self {
            soxr_ptr,
            out_buf: Vec::with_capacity(output_rate as usize / 100), // ensure valid memory ptr
            input_rate,
            output_rate,
            num_channels,
        })
    }

    pub fn push(&mut self, input: &[i16]) -> Result<&[i16], String> {
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
            let error_msg = unsafe { std::ffi::CStr::from_ptr(error) };
            return Err(error_msg.to_string_lossy().to_string());
        }

        let output_samples = odone * self.num_channels as usize;
        Ok(&self.out_buf[..output_samples])
    }

    pub fn flush(&mut self) -> Result<&[i16], String> {
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
            let error_msg = unsafe { std::ffi::CStr::from_ptr(error) };
            return Err(error_msg.to_string_lossy().to_string());
        }

        let error = unsafe { soxr_sys::soxr_clear(self.soxr_ptr) };

        if !error.is_null() {
            let error_msg = unsafe { std::ffi::CStr::from_ptr(error) };
            return Err(error_msg.to_string_lossy().to_string());
        }

        let output_samples = odone * self.num_channels as usize;
        Ok(&self.out_buf[..output_samples])
    }
}

impl Drop for SoxResampler {
    fn drop(&mut self) {
        unsafe {
            soxr_sys::soxr_delete(self.soxr_ptr);
        }
    }
}

fn to_soxr_datatype(datatype: proto::SoxResamplerDataType) -> soxr_sys::soxr_datatype_t {
    match datatype {
        proto::SoxResamplerDataType::SoxrDatatypeInt16i => soxr_sys::soxr_datatype_t_SOXR_INT16_I,
        proto::SoxResamplerDataType::SoxrDatatypeInt16s => soxr_sys::soxr_datatype_t_SOXR_INT16_S,
    }
}

/// Direct-call tests for [`SoxResampler`], paired with the protobuf-driven tests
/// in `crate::migration_tests`. Both sets are written to compile and pass on
/// either side of the uniffi migration, so they can be replayed across it.
///
/// `sox_resampler!` is the only migration seam: the migration renames and
/// re-types the constructor that takes the spec structs, so the macro body is the
/// one thing that differs between commits. The tests bind the resampler to a
/// `mut` local, which suits both a plain `SoxResampler` and an `Arc<Self>`.
#[cfg(test)]
// `mut` is load-bearing before the migration (`push(&mut self)`) and redundant
// after it (`Arc<SoxResampler>`, `push(&self)`).
#[allow(unused_mut)]
mod migration_tests {
    use crate::proto;

    /// Builds a resampler from the same spec structs the FFI request handler uses.
    /// Proto enums are passed through `.into()` so this reads identically before
    /// the migration (identity conversion) and after it (proto -> uniffi enum).
    #[macro_export]
    macro_rules! sox_resampler {
        ($input_rate:expr, $output_rate:expr, $num_channels:expr, $quality:expr) => {
            $crate::server::resampler::SoxResampler::new(
                $input_rate,
                $output_rate,
                $num_channels,
                $crate::server::resampler::IOSpec {
                    input_type: $crate::proto::SoxResamplerDataType::SoxrDatatypeInt16i.into(),
                    output_type: $crate::proto::SoxResamplerDataType::SoxrDatatypeInt16i.into(),
                },
                $crate::server::resampler::QualitySpec { quality: $quality.into(), flags: 0 },
                $crate::server::resampler::RuntimeSpec { num_threads: 1 },
            )
            .unwrap()
        };
    }

    /// 30ms of 48kHz mono in, 30ms of 16kHz mono out, once the filter has been
    /// drained by `flush`.
    #[test]
    fn push_then_flush_conserves_duration() {
        let mut resampler =
            sox_resampler!(48000.0, 16000.0, 1, proto::SoxQualityRecipe::SoxrQualityQuick);
        let frame = vec![1000i16; 480]; // 10ms @ 48kHz

        let mut frames = 0;
        for _ in 0..3 {
            frames += resampler.push(&frame).unwrap().len();
        }
        frames += resampler.flush().unwrap().len();

        // 3 x 10ms @ 48kHz == 480 frames @ 16kHz, give or take the filter delay.
        assert!((frames as i64 - 480).abs() <= 8, "got {frames} frames, expected ~480");
    }

    /// A steady level comes out at the same level, i.e. the sample data really is
    /// being resampled rather than reinterpreted or truncated.
    #[test]
    fn steady_level_survives_resampling() {
        let mut resampler =
            sox_resampler!(48000.0, 16000.0, 1, proto::SoxQualityRecipe::SoxrQualityVeryhigh);
        let frame = vec![8000i16; 4800]; // 100ms @ 48kHz

        resampler.push(&frame).unwrap(); // warm up past the filter ramp
        let output = resampler.push(&frame).unwrap();

        assert!(output.len() > 1000, "expected ~1600 frames, got {}", output.len());
        for (i, sample) in output.iter().enumerate() {
            assert!((*sample as i32 - 8000).abs() < 80, "sample {i} is {sample}, expected ~8000");
        }
    }

    /// Interleaved channels are resampled independently: a stereo frame of
    /// (+8000, -8000) must not average out into silence.
    #[test]
    fn interleaved_channels_stay_separate() {
        let mut resampler =
            sox_resampler!(48000.0, 24000.0, 2, proto::SoxQualityRecipe::SoxrQualityQuick);
        let frame: Vec<i16> = std::iter::repeat([8000, -8000]).take(4800).flatten().collect();

        resampler.push(&frame).unwrap(); // warm up past the filter ramp
        let output = resampler.push(&frame).unwrap();

        assert_eq!(output.len() % 2, 0, "output is not a whole number of stereo frames");
        assert!(output.len() > 1000, "expected ~4800 samples, got {}", output.len());
        for (i, out_frame) in output.chunks(2).enumerate() {
            assert!(out_frame[0] > 7000, "frame {i} left is {}, expected ~8000", out_frame[0]);
            assert!(out_frame[1] < -7000, "frame {i} right is {}, expected ~-8000", out_frame[1]);
        }
    }
}
