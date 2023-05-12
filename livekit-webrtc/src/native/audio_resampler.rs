use cxx::UniquePtr;
use webrtc_sys::audio_resampler as sys_ar;

pub struct AudioResampler {
    sys_handle: UniquePtr<sys_ar::ffi::AudioResampler>,
}

impl Default for AudioResampler {
    fn default() -> Self {
        Self {
            sys_handle: sys_ar::ffi::create_audio_resampler(),
        }
    }
}

impl AudioResampler {
    pub fn remix_and_resample(
        &mut self,
        src: &[i16],
        samples_per_channel: u32,
        num_channels: u32,
        sample_rate: u32,
        dst_num_channels: u32,
        dst_sample_rate: u32,
    ) -> &[i16] {
        unsafe {
            let len = self.sys_handle.pin_mut().remix_and_resample(
                src.as_ptr(),
                samples_per_channel as usize,
                num_channels as usize,
                sample_rate as i32,
                dst_num_channels as usize,
                dst_sample_rate as i32,
            );

            std::slice::from_raw_parts(self.sys_handle.data(), len / 2)
        }
    }
}
