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

use cxx::UniquePtr;
use webrtc_sys::apm::ffi as sys_apm;

use crate::{RtcError, RtcErrorType};

pub struct AudioProcessingModule {
    sys_handle: UniquePtr<sys_apm::AudioProcessingModule>,
}

impl AudioProcessingModule {
    pub fn new(
        echo_canceller_enabled: bool,
        gain_controller_enabled: bool,
        high_pass_filter_enabled: bool,
        noise_suppression_enabled: bool,
    ) -> Self {
        Self {
            sys_handle: unsafe {
                sys_apm::create_apm(
                    echo_canceller_enabled,
                    gain_controller_enabled,
                    high_pass_filter_enabled,
                    noise_suppression_enabled,
                )
            },
        }
    }

    pub fn process_stream(
        &mut self,
        data: &mut [i16],
        sample_rate: i32,
        num_channels: i32,
    ) -> Result<(), RtcError> {
        let samples_count = (sample_rate as usize / 100) * num_channels as usize;
        assert_eq!(data.len(), samples_count, "slice must have 10ms worth of samples");

        if unsafe {
            // using the same slice for src and dst is safe
            self.sys_handle.pin_mut().process_stream(
                data.as_mut_ptr(),
                data.len(),
                data.as_mut_ptr(),
                data.len(),
                sample_rate,
                num_channels,
            )
        } == 0
        {
            Ok(())
        } else {
            Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: "Failed to process stream".to_string(),
            })
        }
    }

    pub fn process_reverse_stream(
        &mut self,
        data: &mut [i16],
        sample_rate: i32,
        num_channels: i32,
    ) -> Result<(), RtcError> {
        let samples_count = (sample_rate as usize / 100) * num_channels as usize;
        assert_eq!(data.len(), samples_count, "slice must have 10ms worth of samples");

        if unsafe {
            // using the same slice for src and dst is safe
            self.sys_handle.pin_mut().process_reverse_stream(
                data.as_mut_ptr(),
                data.len(),
                data.as_mut_ptr(),
                data.len(),
                sample_rate,
                num_channels,
            )
        } == 0
        {
            Ok(())
        } else {
            Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: "Failed to process reverse stream".to_string(),
            })
        }
    }

    pub fn set_stream_delay_ms(&mut self, delay_ms: i32) -> Result<(), RtcError> {
        if self.sys_handle.pin_mut().set_stream_delay_ms(delay_ms) == 0 {
            Ok(())
        } else {
            Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: "Failed to set stream delay".to_string(),
            })
        }
    }
}
