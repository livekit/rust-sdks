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

use crate::audio_frame::AudioFrame;
use crate::sys;
use std::sync::Arc;

pub struct AudioMixer {
    ffi: sys::RefCounted<sys::lkAudioMixer>,
}

pub trait AudioMixerSource {
    fn ssrc(&self) -> i32;
    fn preferred_sample_rate(&self) -> u32;
    fn get_audio_frame_with_info(&self, target_sample_rate: u32) -> Option<AudioFrame>;
}

struct AudioMixerSourceImpl<T> {
    inner: T,
}

impl<T: AudioMixerSource> AudioMixerSource for AudioMixerSourceImpl<T> {
    fn ssrc(&self) -> i32 {
        self.inner.ssrc()
    }

    fn preferred_sample_rate(&self) -> u32 {
        self.inner.preferred_sample_rate()
    }

    fn get_audio_frame_with_info(
        &self,
        target_sample_rate: i32,
        native_frame: sys::NativeAudioFrame,
    ) -> sys::lkAudioFrameInfo {
        if let Some(frame) = self.inner.get_audio_frame_with_info(target_sample_rate as u32) {
            let samples_count = (frame.sample_rate as usize / 100) as usize;
            assert_eq!(
                frame.sample_rate, target_sample_rate as u32,
                "sample rate must match target_sample_rate"
            );
            assert_eq!(
                frame.samples_per_channel as usize, samples_count,
                "frame must contain 10ms of samples"
            );
            assert_eq!(
                frame.data.len(),
                samples_count * frame.num_channels as usize,
                "slice must contain 10ms of samples"
            );

            unsafe {
                native_frame.update_frame(
                    0,
                    frame.data.as_ptr(),
                    frame.samples_per_channel as usize,
                    frame.sample_rate as i32,
                    frame.num_channels as usize,
                );
            }
            return sys::lkAudioFrameInfo::Normal;
        } else {
            return sys::lkAudioFrameInfo::Muted;
        }
    }
}

impl AudioMixer {
    pub fn new() -> Self {
        let ffi = unsafe { sys::lkCreateAudioMixer() };
        Self { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn add_source(&mut self, source: impl AudioMixerSource + 'static) {
        let source_impl = AudioMixerSourceImpl { inner: source };
        let wrapper = Box::new(sys::AudioMixerSourceWrapper::new(Arc::new(source_impl)));
        unsafe {
            sys::lkAudioMixerAddSource(self.ffi.as_ptr(), wrapper);
        }
    }

    pub fn remove_source(&mut self, ssrc: i32) {
        unsafe {
            sys::lkAudioMixerRemoveSource(self.ffi.as_ptr(), ssrc);
        }
    }

    pub fn mix(&mut self, num_channels: usize) -> &[i16] {
        unsafe {
            let len =
                sys::lkAudioMixerMixFrame(self.ffi.as_ptr(), num_channels.try_into().unwrap());
            let lk_data = sys::lkAudioMixerGetMixedFrame(self.ffi.as_ptr(), len);
            let data = sys::RefCountedData::from_native(lk_data);

            std::slice::from_raw_parts(
                data.as_ptr() as *const i16,
                data.len() / std::mem::size_of::<i16>(),
            )
        }
    }
}
