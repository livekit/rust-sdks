// Copyright 2023 LiveKit, Inc.
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

use cxx::UniquePtr;
use webrtc_sys::aec::ffi as sys_aec;

pub struct Aec {
    sys_handle: UniquePtr<sys_aec::Aec>,
    sample_rate: i32,
    num_channels: i32,
}

impl Aec {
    pub fn new(sample_rate: i32, num_channels: i32) -> Self {
        Self {
            sys_handle: sys_aec::create_aec(sample_rate, num_channels),
            sample_rate,
            num_channels,
        }
    }

    pub fn cancel_echo(&mut self, capture: &mut [i16], render: &[i16]) {
        let required_samples = (self.sample_rate as usize / 100) * self.num_channels as usize;
        assert_eq!(
            capture.len(),
            required_samples,
            "Capture slice must have 10ms worth of samples"
        );
        assert_eq!(render.len(), required_samples, "Render slice must have 10ms worth of samples");

        unsafe {
            self.sys_handle.pin_mut().cancel_echo(
                capture.as_mut_ptr(),
                capture.len(),
                render.as_ptr(),
                render.len(),
            );
        }
    }
}
