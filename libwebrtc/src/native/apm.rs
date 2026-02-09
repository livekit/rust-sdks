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

use crate::impl_thread_safety;
use crate::sys;
use crate::{RtcError, RtcErrorType};

pub struct AudioProcessingModule {
    ffi: sys::RefCounted<sys::lkAudioProcessingModule>,
}

impl AudioProcessingModule {
    pub fn new(
        echo_canceller_enabled: bool,
        gain_controller_enabled: bool,
        high_pass_filter_enabled: bool,
        noise_suppression_enabled: bool,
    ) -> Self {
        unsafe {
            let ffi = sys::lkAudioProcessingModuleCreate(
                echo_canceller_enabled,
                gain_controller_enabled,
                high_pass_filter_enabled,
                noise_suppression_enabled,
            );
            Self { ffi: sys::RefCounted::from_raw(ffi) }
        }
    }

    pub fn process_stream(
        &mut self,
        data: &mut [i16],
        sample_rate: i32,
        num_channels: i32,
    ) -> Result<(), RtcError> {
        let samples_per_10ms = (sample_rate as usize / 100) * num_channels as usize;
        assert!(
            data.len().is_multiple_of(samples_per_10ms) && data.len() >= samples_per_10ms,
            "slice must have a multiple of 10ms worth of samples"
        );

        unsafe {
            if sys::lkAudioProcessingModuleProcessStream(
                self.ffi.as_ptr(),
                data.as_mut_ptr(),
                data.len() as u32,
                data.as_mut_ptr(),
                data.len() as u32,
                sample_rate,
                num_channels,
            ) == 0
            {
                Ok(())
            } else {
                Err(RtcError {
                    error_type: RtcErrorType::OperationError,
                    message: "Failed to process stream".to_string(),
                })
            }
        }
    }

    pub fn process_reverse_stream(
        &mut self,
        data: &mut [i16],
        sample_rate: i32,
        num_channels: i32,
    ) -> Result<(), RtcError> {
        let samples_per_10ms = (sample_rate as usize / 100) * num_channels as usize;
        assert!(
            data.len().is_multiple_of(samples_per_10ms) && data.len() >= samples_per_10ms,
            "slice must have a multiple of 10ms worth of samples"
        );

        unsafe {
            if sys::lkAudioProcessingModuleProcessReverseStream(
                self.ffi.as_ptr(),
                data.as_mut_ptr(),
                data.len() as u32,
                data.as_mut_ptr(),
                data.len() as u32,
                sample_rate,
                num_channels,
            ) == 0
            {
                Ok(())
            } else {
                Err(RtcError {
                    error_type: RtcErrorType::OperationError,
                    message: "Failed to process reverse stream".to_string(),
                })
            }
        }
    }

    pub fn set_stream_delay_ms(
        self: &mut AudioProcessingModule,
        delay: i32,
    ) -> Result<(), RtcError> {
        unsafe {
            if sys::lkAudioProcessingModuleSetStreamDelayMs(self.ffi.as_ptr(), delay) == 0 {
                Ok(())
            } else {
                Err(RtcError {
                    error_type: RtcErrorType::OperationError,
                    message: "Failed to set stream delay".to_string(),
                })
            }
        }
    }
}

impl_thread_safety!(AudioProcessingModule, Send + Sync);
