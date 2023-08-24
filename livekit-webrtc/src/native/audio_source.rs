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

use crate::{audio_frame::AudioFrame, audio_source::AudioSourceOptions, RtcError, RtcErrorType};
use cxx::SharedPtr;
use std::{sync::Arc, time::Duration};
use tokio::{sync::Mutex as AsyncMutex, time::interval};
use webrtc_sys::audio_track as sys_at;

#[derive(Clone)]
pub struct NativeAudioSource {
    sys_handle: SharedPtr<sys_at::ffi::AudioTrackSource>,
    inner: Arc<AsyncMutex<AudioSourceInner>>,
    sample_rate: u32,
    num_channels: u32,
    samples_10ms: usize,
}

#[derive(Default)]
struct AudioSourceInner {
    buf: Box<[i16]>,
    len: usize, // Data available inside buf
}

impl NativeAudioSource {
    pub fn new(
        options: AudioSourceOptions,
        sample_rate: u32,
        num_channels: u32,
    ) -> NativeAudioSource {
        let samples_10ms = (sample_rate / 100 * num_channels) as usize;

        Self {
            sys_handle: sys_at::ffi::new_audio_track_source(options.into()),
            inner: Arc::new(AsyncMutex::new(AudioSourceInner {
                buf: vec![0; samples_10ms].into_boxed_slice(),
                len: 0,
            })),
            sample_rate,
            num_channels,
            samples_10ms,
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

    pub async fn capture_frame(&self, frame: &AudioFrame<'_>) -> Result<(), RtcError> {
        if self.sample_rate != frame.sample_rate || self.num_channels != frame.num_channels {
            return Err(RtcError {
                error_type: RtcErrorType::InvalidState,
                message: "sample_rate and num_channels don't match".to_owned(),
            });
        }

        let mut inner = self.inner.lock().await;
        let mut interval = interval(Duration::from_millis(10));
        interval.tick().await;

        let mut offset = 0;
        loop {
            let data_len = frame.data.len();
            let buf_len = inner.len;
            let remaining_len = buf_len + data_len - offset;

            if remaining_len < self.samples_10ms {
                if remaining_len != 0 {
                    let remaining_data = &frame.data[(data_len - remaining_len)..];
                    inner.len = remaining_len;
                    inner.buf[..remaining_len].copy_from_slice(remaining_data);
                }

                break;
            }

            // if data is available inside inner.buf, use it
            let data = if buf_len > 0 {
                let missing = &mut inner.buf[buf_len..];
                let data = &frame.data[..(self.samples_10ms - buf_len)];

                missing.copy_from_slice(data); // Fill the missing with data coming from the frame

                offset += buf_len;
                inner.len = 0;
                &inner.buf
            } else {
                offset += self.samples_10ms;
                &frame.data[offset..(offset + self.samples_10ms)]
            };

            self.sys_handle.on_captured_frame(
                data,
                self.sample_rate as i32,
                self.num_channels as usize,
                self.samples_10ms / self.num_channels as usize,
            );

            interval.tick().await;
        }

        Ok(())
    }
}

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
