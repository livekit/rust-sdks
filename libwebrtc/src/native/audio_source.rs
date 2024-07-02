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

use std::{sync::Arc, time::Duration};

use cxx::SharedPtr;
use livekit_runtime::interval;
use tokio::sync::{
    mpsc::{self, error::TryRecvError},
    Mutex as AsyncMutex,
};
use webrtc_sys::audio_track as sys_at;

use crate::{audio_frame::AudioFrame, audio_source::AudioSourceOptions, RtcError, RtcErrorType};

const BUFFER_SIZE_MS: usize = 50;

#[derive(Clone)]
pub struct NativeAudioSource {
    sys_handle: SharedPtr<sys_at::ffi::AudioTrackSource>,
    inner: Arc<AsyncMutex<AudioSourceInner>>,
    sample_rate: u32,
    num_channels: u32,
    samples_10ms: usize,
    // whether to queue audio frames or send them immediately
    // defaults to true
    enable_queue: bool,
    po_tx: mpsc::Sender<Vec<i16>>,
}

struct AudioSourceInner {
    buf: Box<[i16]>,

    // Amount of data from the previous frame that hasn't been sent to the libwebrtc source
    // (because it requires 10ms of data)
    len: usize,
}

impl NativeAudioSource {
    pub fn new(
        options: AudioSourceOptions,
        sample_rate: u32,
        num_channels: u32,
        enable_queue: Option<bool>,
    ) -> NativeAudioSource {
        let samples_10ms = (sample_rate / 100 * num_channels) as usize;
        let (po_tx, mut po_rx) = mpsc::channel(BUFFER_SIZE_MS / 10);

        let source = Self {
            sys_handle: sys_at::ffi::new_audio_track_source(options.into()),
            inner: Arc::new(AsyncMutex::new(AudioSourceInner {
                buf: vec![0; samples_10ms].into_boxed_slice(),
                len: 0,
            })),
            sample_rate,
            num_channels,
            samples_10ms,
            enable_queue: enable_queue.unwrap_or(true),
            po_tx,
        };

        livekit_runtime::spawn({
            let source = source.clone();
            async move {
                let mut interval = interval(Duration::from_millis(10));
                interval.set_missed_tick_behavior(livekit_runtime::MissedTickBehavior::Delay);
                let blank_data = vec![0; samples_10ms];
                let enable_queue = source.enable_queue;

                loop {
                    if enable_queue {
                        interval.tick().await;
                    }

                    let frame = po_rx.try_recv();
                    if let Err(TryRecvError::Disconnected) = frame {
                        break;
                    }

                    if let Err(TryRecvError::Empty) = frame {
                        if enable_queue {
                            source.sys_handle.on_captured_frame(
                                &blank_data,
                                sample_rate,
                                num_channels,
                                blank_data.len() / num_channels as usize,
                            );
                        }
                        continue;
                    }

                    let frame = frame.unwrap();
                    source.sys_handle.on_captured_frame(
                        &frame,
                        sample_rate,
                        num_channels,
                        frame.len() / num_channels as usize,
                    );
                }
            }
        });

        source
    }

    pub fn sys_handle(&self) -> SharedPtr<sys_at::ffi::AudioTrackSource> {
        self.sys_handle.clone()
    }

    pub fn set_audio_options(&self, options: AudioSourceOptions) {
        self.sys_handle.set_audio_options(&sys_at::ffi::AudioSourceOptions::from(options))
    }

    pub fn audio_options(&self) -> AudioSourceOptions {
        self.sys_handle.audio_options().into()
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn num_channels(&self) -> u32 {
        self.num_channels
    }

    pub fn enable_queue(&self) -> bool {
        self.enable_queue
    }

    pub async fn capture_frame(&self, frame: &AudioFrame<'_>) -> Result<(), RtcError> {
        if self.sample_rate != frame.sample_rate || self.num_channels != frame.num_channels {
            return Err(RtcError {
                error_type: RtcErrorType::InvalidState,
                message: "sample_rate and num_channels don't match".to_owned(),
            });
        }

        let mut inner = self.inner.lock().await;
        let mut samples = 0;
        // split frames into 10ms chunks
        loop {
            let remaining_samples = frame.data.len() - samples;
            if remaining_samples == 0 {
                break;
            }

            if (inner.len != 0 && remaining_samples > 0) || remaining_samples < self.samples_10ms {
                let missing_len = self.samples_10ms - inner.len;
                let to_add = missing_len.min(remaining_samples);
                let start = inner.len;
                inner.buf[start..start + to_add]
                    .copy_from_slice(&frame.data[samples..samples + to_add]);
                inner.len += to_add;
                samples += to_add;

                if inner.len == self.samples_10ms {
                    let data = inner.buf.clone().to_vec();
                    let _ = self.po_tx.send(data).await;
                    inner.len = 0;
                }
                continue;
            }

            if remaining_samples >= self.samples_10ms {
                // TODO(theomonnom): avoid copying
                let data = frame.data[samples..samples + self.samples_10ms].to_vec();
                let _ = self.po_tx.send(data).await;
                samples += self.samples_10ms;
            }
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
