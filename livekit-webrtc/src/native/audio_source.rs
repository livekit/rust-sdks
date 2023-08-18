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

use crate::{audio_frame::AudioFrame, audio_source::AudioSourceOptions};
use cxx::SharedPtr;
use parking_lot::Mutex;
use std::sync::Arc;
use webrtc_sys::audio_track as sys_at;

impl From<sys_at::ffi::AudioSourceOptions> for AudioSourceOptions {
    fn from(options: sys_at::ffi::AudioSourceOptions) -> Self {
        Self {
            echo_cancellation: options.echo_cancellation,
            noise_suppression: options.noise_suppression,
            auto_gain_control: options.auto_gain_control,
        }
    }
}

impl From<AudioSourceOptions> for sys_at::ffi::AudioSourceOptions {
    fn from(options: AudioSourceOptions) -> Self {
        Self {
            echo_cancellation: options.echo_cancellation,
            noise_suppression: options.noise_suppression,
            auto_gain_control: options.auto_gain_control,
        }
    }
}

#[derive(Clone)]
pub struct NativeAudioSource {
    sys_handle: SharedPtr<sys_at::ffi::AudioTrackSource>,
    inner: Arc<Mutex<AudioSourceInner>>,
}

#[derive(Default)]
struct AudioSourceInner {
    buf: Vec<i16>,
    offset: usize,
    sample_rate: u32,
    num_channels: u32,
}

impl NativeAudioSource {
    pub fn new(options: AudioSourceOptions) -> NativeAudioSource {
        Self {
            sys_handle: sys_at::ffi::new_audio_track_source(options.into()),
            inner: Default::default(),
        }
    }

    pub fn sys_handle(&self) -> SharedPtr<sys_at::ffi::AudioTrackSource> {
        self.sys_handle.clone()
    }

    pub fn set_audio_options(&self, options: AudioSourceOptions) {
        self.sys_handle
            .set_audio_options(&sys_at::ffi::AudioSourceOptions::from(options))
    }

    pub fn audio_options(&self) -> AudioSourceOptions {
        self.sys_handle.audio_options().into()
    }

    pub fn capture_frame(&self, frame: &AudioFrame) {
        let mut inner = self.inner.lock();
        let samples_10ms = (frame.sample_rate / 100 * frame.num_channels) as usize;
        if inner.sample_rate != frame.sample_rate || inner.num_channels != frame.num_channels {
            inner.buf.resize(samples_10ms as usize, 0);
            inner.offset = 0;
            inner.sample_rate = frame.sample_rate;
            inner.num_channels = frame.num_channels;
        }

        // Split the frame into 10ms chunks
        let mut i = 0;
        loop {
            let buf_offset = inner.offset;
            let remaining_data = frame.data.len() - i; // Remaining data to read inside the frame
            let needed_data = samples_10ms - buf_offset; // Needed data of "frame.data" to make a complete 10ms from inner.buf
            if remaining_data < needed_data {
                if remaining_data > 0 {
                    // Not enough data to make a complete 10ms frame, store the remaining data inside inner.buf
                    // It'll be used on the next capture.
                    inner.buf[buf_offset..buf_offset + remaining_data]
                        .copy_from_slice(&frame.data[i..]);
                    inner.offset += remaining_data;
                }

                break;
            }

            let data = if inner.offset != 0 {
                // Use the data from the previous capture
                let data = &mut inner.buf[buf_offset..];
                data.copy_from_slice(&frame.data[i..i + needed_data]);
                inner.offset = 0;
                &inner.buf
            } else {
                &frame.data[i..i + samples_10ms]
            };

            self.sys_handle.on_captured_frame(
                data,
                frame.sample_rate as i32,
                frame.num_channels as usize,
                samples_10ms / frame.num_channels as usize,
            );

            i += needed_data;
        }
    }
}
