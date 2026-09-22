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
