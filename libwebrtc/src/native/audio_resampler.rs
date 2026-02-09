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

use crate::sys;

pub struct AudioResampler {
    ffi: sys::RefCounted<sys::lkAudioResampler>,
}

impl Default for AudioResampler {
    fn default() -> Self {
        unsafe {
            let ffi = sys::lkAudioResamplerCreate();
            Self { ffi: sys::RefCounted::from_raw(ffi) }
        }
    }
}

impl AudioResampler {
    pub fn remix_and_resample<'a>(
        &'a mut self,
        src: &[i16],
        samples_per_channel: u32,
        num_channels: u32,
        sample_rate: u32,
        dst_num_channels: u32,
        dst_sample_rate: u32,
    ) -> &'a [i16] {
        assert!(src.len() >= (samples_per_channel * num_channels) as usize, "src buffer too small");
        unsafe {
            let len = sys::lkAudioResamplerResample(
                self.ffi.as_ptr(),
                src.as_ptr(),
                samples_per_channel,
                num_channels,
                sample_rate,
                dst_num_channels,
                dst_sample_rate,
            );
            let data_ptr = sys::lkAudioResamplerGetData(self.ffi.as_ptr());
            std::slice::from_raw_parts(data_ptr, len as usize / 2)
        }
    }
}
