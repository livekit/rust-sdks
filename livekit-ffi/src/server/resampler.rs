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
    /// Creates a new SoxResampler using soxr's default quality and runtime options.
    /// The provided `QualitySpec` and `RuntimeSpec` are ignored and null pointers are passed
    /// to `soxr_create` to let soxr choose its defaults.
    pub fn new(
        input_rate: f64,
        output_rate: f64,
        num_channels: u32,
        io_spec: IOSpec,
        _quality_spec: QualitySpec, // ignored – using default soxr options
        _runtime_spec: RuntimeSpec, // ignored – using default soxr options
    ) -> Result<Self, String> {
        let mut err: *mut *const c_char = std::ptr::null_mut();

        let soxr_ptr = unsafe {
            // Create io_spec from our types.
            let io_spec = soxr_sys::soxr_io_spec(
                to_soxr_datatype(io_spec.input_type),
                to_soxr_datatype(io_spec.output_type),
            );

            // Pass null pointers for quality and runtime specs so that
            // soxr will use its internal default options.
            soxr_sys::soxr_create(
                input_rate,
                output_rate,
                num_channels,
                err,
                &io_spec,
                std::ptr::null(), // default quality
                std::ptr::null(), // default runtime
            )
        };

        if !err.is_null() || soxr_ptr.is_null() {
            let error_msg = unsafe { std::ffi::CStr::from_ptr(*err) };
            return Err(error_msg.to_string_lossy().to_string());
        }

        Ok(Self { soxr_ptr, out_buf: Vec::new(), input_rate, output_rate, num_channels })
    }

    /// Processes the input buffer and returns the resampled output.
    /// This version verifies that the input length is a multiple of the number of channels
    /// and uses valid pointers for tracking the number of frames consumed and produced.
    pub fn push(&mut self, input: &[i16]) -> Result<&[i16], String> {
        let input_length = input.len() / self.num_channels as usize;
        let ratio = self.output_rate / self.input_rate;
        let delay = unsafe { soxr_sys::soxr_delay(self.soxr_ptr) };

        // Estimate maximum output frames: processed frames + delay + an extra frame.
        let max_out_len =
            (input_length as f64 * ratio).ceil() as usize + (delay.ceil() as usize) + 1;

        let required_output_size = max_out_len * self.num_channels as usize;
        if self.out_buf.len() < required_output_size {
            self.out_buf.resize(required_output_size, 0);
        }

        // Using valid pointers for both consumed input (idone) and produced output (odone)
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

    /// Flushes the internal state, processing any remaining data.
    /// Passes null for the input pointer and for the idone parameter (since it is not needed).
    pub fn flush(&mut self) -> Result<&[i16], String> {
        let mut odone: usize = 0;
        let error = unsafe {
            soxr_sys::soxr_process(
                self.soxr_ptr,
                std::ptr::null(), // no more input
                0,
                std::ptr::null_mut(), // no need to know how many were consumed
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

        Ok(&self.out_buf[..odone])
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
