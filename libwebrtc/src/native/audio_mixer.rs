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

use crate::{audio_frame::AudioFrame, sys};
use std::sync::Arc;

pub struct AudioMixer {
    ffi: sys::RefCounted<sys::lkAudioMixer>,
}

pub trait AudioMixerSource {
    fn ssrc(&self) -> i32;
    fn preferred_sample_rate(&self) -> u32;
    fn get_audio_frame_with_info(&'_ self, target_sample_rate: u32) -> Option<AudioFrame<'_>>;
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

    fn get_audio_frame_with_info(&'_ self, target_sample_rate: u32) -> Option<AudioFrame<'_>> {
        self.inner.get_audio_frame_with_info(target_sample_rate)
    }
}

pub static SYS_AUDIO_MIXER_CALLBACKS: sys::lkAudioMixerSourceCallback =
    sys::lkAudioMixerSourceCallback {
        getSsrc: Some(AudioMixer::audio_mixer_source_get_ssrc),
        preferredSampleRate: Some(AudioMixer::audio_mixer_source_get_preferred_sample_rate),
        getAudioFrameWithInfo: Some(AudioMixer::audio_mixer_source_get_audio_frame_with_info),
    };

impl Default for AudioMixer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioMixer {
    pub fn new() -> Self {
        let ffi = unsafe { sys::lkCreateAudioMixer() };
        Self { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn add_source(&mut self, source: impl AudioMixerSource + 'static) {
        let source_impl = AudioMixerSourceImpl { inner: source };
        let wrapper = Box::new(Arc::new(source_impl));
        unsafe {
            sys::lkAudioMixerAddSource(
                self.ffi.as_ptr(),
                &SYS_AUDIO_MIXER_CALLBACKS,
                Box::into_raw(wrapper) as *mut _,
            );
        }
    }

    pub fn remove_source(&mut self, ssrc: i32) {
        unsafe {
            sys::lkAudioMixerRemoveSource(self.ffi.as_ptr(), ssrc);
        }
    }

    pub fn mix(&mut self, num_channels: usize) -> &[i16] {
        unsafe {
            let len = sys::lkAudioMixerMixFrame(self.ffi.as_ptr(), num_channels as u32);
            let data_ptr = sys::lkAudioMixerGetData(self.ffi.as_ptr());
            std::slice::from_raw_parts(data_ptr, len as usize)
        }
    }
    pub extern "C" fn audio_mixer_source_get_ssrc(userdata: *mut ::std::os::raw::c_void) -> i32 {
        let source =
            unsafe { &*(userdata as *const AudioMixerSourceImpl<Box<dyn AudioMixerSource>>) };
        source.inner.ssrc()
    }

    pub extern "C" fn audio_mixer_source_get_preferred_sample_rate(
        userdata: *mut ::std::os::raw::c_void,
    ) -> i32 {
        let source =
            unsafe { &*(userdata as *const AudioMixerSourceImpl<Box<dyn AudioMixerSource>>) };
        source.inner.preferred_sample_rate() as i32
    }

    pub unsafe extern "C" fn audio_mixer_source_get_audio_frame_with_info(
        target_sample_rate: u32,
        native_frame: *mut sys::lkNativeAudioFrame,
        userdata: *mut ::std::os::raw::c_void,
    ) -> sys::lkAudioFrameInfo {
        let source =
            unsafe { &*(userdata as *const AudioMixerSourceImpl<Box<dyn AudioMixerSource>>) };
        if let Some(frame) = source.inner.get_audio_frame_with_info(target_sample_rate) {
            let samples_count = (frame.sample_rate as usize / 100);
            assert_eq!(
                frame.sample_rate, target_sample_rate,
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
                sys::lkNativeAudioFrameUpdateFrame(
                    native_frame,
                    0,
                    frame.data.as_ptr(),
                    frame.samples_per_channel,
                    frame.sample_rate.try_into().unwrap(),
                    frame.num_channels,
                );
            }
            sys::lkAudioFrameInfo::AUDIO_FRAME_INFO_NORMAL
        } else {
            sys::lkAudioFrameInfo::AUDIO_FRAME_INFO_MUTE
        }
    }
}
