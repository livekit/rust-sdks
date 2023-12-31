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
use tokio::{
    sync::{oneshot, Mutex as AsyncMutex, MutexGuard},
    time::{interval, Instant, MissedTickBehavior},
};
use webrtc_sys::audio_track as sys_at;

use crate::{audio_frame::AudioFrame, audio_source::AudioSourceOptions, RtcError, RtcErrorType};

#[derive(Clone)]
pub struct NativeAudioSource {
    sys_handle: SharedPtr<sys_at::ffi::AudioTrackSource>,
    inner: Arc<AsyncMutex<AudioSourceInner>>,
    sample_rate: u32,
    num_channels: u32,
    samples_10ms: usize,
    _close_tx: Arc<oneshot::Sender<()>>,
}

struct AudioSourceInner {
    buf: Box<[i16]>,

    last_capture: Option<Instant>,

    // Amount of data from the previous frame that hasn't been sent to the libwebrtc source
    // (because it requires 10ms of data)
    len: usize,

    // Amount of data that have been read inside the current AudioFrame
    read_offset: usize,

    interval: Option<tokio::time::Interval>,
}

impl NativeAudioSource {
    pub fn new(
        options: AudioSourceOptions,
        sample_rate: u32,
        num_channels: u32,
    ) -> NativeAudioSource {
        let samples_10ms = (sample_rate / 100 * num_channels) as usize;
        let (close_tx, mut close_rx) = oneshot::channel();

        let source = Self {
            sys_handle: sys_at::ffi::new_audio_track_source(options.into()),
            inner: Arc::new(AsyncMutex::new(AudioSourceInner {
                buf: vec![0; samples_10ms].into_boxed_slice(),
                last_capture: None,
                len: 0,
                read_offset: 0,
                interval: None, // interval must be created from a tokio runtime context
            })),
            sample_rate,
            num_channels,
            samples_10ms,
            _close_tx: Arc::new(close_tx),
        };

        tokio::spawn({
            let source = source.clone();
            async move {
                let mut interval = interval(Duration::from_millis(10));
                let data = vec![0; samples_10ms];

                loop {
                    tokio::select! {
                        _ = &mut close_rx => break,
                        _ = interval.tick() => {
                            let inner = source.inner.lock().await;
                            if let Some(last_capture) = inner.last_capture {
                                if last_capture.elapsed() < Duration::from_millis(20) {
                                    continue;
                                }
                            }

                            source.sys_handle.on_captured_frame(
                                &data,
                                sample_rate,
                                num_channels,
                                sample_rate as usize / 100,
                            );
                        }
                    }
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

    // Implemented inside another functions to allow unit testing
    fn next_frame<'a>(
        &self,
        inner: &'a mut MutexGuard<'_, AudioSourceInner>, /* The lock musts be guarded by
                                                          * capture_frame */
        frame: &'a AudioFrame<'_>,
    ) -> Option<&'a [i16]> {
        let available_data = inner.len + frame.data.len() - inner.read_offset;
        if available_data >= self.samples_10ms {
            Some(if inner.len != 0 {
                // Read 10ms frame from inner.buf AND frame.data
                let missing_len = self.samples_10ms - inner.len;
                let start = inner.len;
                inner.buf[start..].copy_from_slice(&frame.data[..missing_len]);
                inner.read_offset += missing_len;
                inner.len = 0;
                &inner.buf
            } else {
                // Read 10ms frame only from frame.data
                let start = inner.read_offset;
                let end = start + self.samples_10ms;
                inner.read_offset += self.samples_10ms;
                &frame.data[start..end]
            })
        } else {
            // Save to buf and wait for the next capture_frame to give enough data to complete a
            // 10ms frame
            let remaining_data = frame.data.len() - inner.read_offset; // remaining data from frame.data
            let start = inner.len;
            let end = start + remaining_data;
            let start2 = frame.data.len() - remaining_data;
            inner.buf[start..end].copy_from_slice(&frame.data[start2..]);
            inner.len += remaining_data;
            inner.read_offset = 0;
            None
        }
    }

    pub async fn capture_frame(&self, frame: &AudioFrame<'_>) -> Result<(), RtcError> {
        if self.sample_rate != frame.sample_rate || self.num_channels != frame.num_channels {
            return Err(RtcError {
                error_type: RtcErrorType::InvalidState,
                message: "sample_rate and num_channels don't match".to_owned(),
            });
        }

        let mut inner = self.inner.lock().await;
        let mut interval = inner.interval.take().unwrap_or_else(|| {
            let mut interval = interval(Duration::from_millis(10));
            interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
            interval
        });

        loop {
            let Some(data) = self.next_frame(&mut inner, frame) else {
                inner.interval = Some(interval); // Is there a better way to avoid double mut reference than taking the Option?
                break;
            };

            let last_capture = interval.tick().await;

            // samples per channel = number of frames
            let samples_per_channel = data.len() / self.num_channels as usize;
            self.sys_handle.on_captured_frame(
                data,
                self.sample_rate,
                self.num_channels,
                samples_per_channel,
            );

            inner.last_capture = Some(last_capture);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn split_frames() {
        let source = NativeAudioSource::new(AudioSourceOptions::default(), 48000, 2);
        let samples_count =
            source.sample_rate() as usize / 1000 * 20 * source.num_channels() as usize; // 20ms

        let audio_frame = AudioFrame {
            data: vec![0; samples_count].into(),
            sample_rate: source.sample_rate(),
            num_channels: source.num_channels(),
            samples_per_channel: samples_count as u32 / source.num_channels(),
        };

        let mut inner = source.inner.lock().await;

        assert!(source.next_frame(&mut inner, &audio_frame).is_some());
        assert!(source.next_frame(&mut inner, &audio_frame).is_some());
        assert!(source.next_frame(&mut inner, &audio_frame).is_none());
    }

    #[tokio::test]
    async fn buffer_is_used() {
        let source = NativeAudioSource::new(AudioSourceOptions::default(), 48000, 2);
        let samples_15ms =
            source.sample_rate() as usize / 1000 * 15 * source.num_channels() as usize;

        let audio_frame = AudioFrame {
            data: vec![0; samples_15ms].into(),
            sample_rate: source.sample_rate(),
            num_channels: source.num_channels(),
            samples_per_channel: samples_15ms as u32 / source.num_channels(),
        };

        let mut inner = source.inner.lock().await;

        assert!(source.next_frame(&mut inner, &audio_frame).is_some());
        assert!(source.next_frame(&mut inner, &audio_frame).is_none());

        let samples_5ms = source.sample_rate() as usize / 1000 * 5 * source.num_channels() as usize;
        assert_eq!(inner.len, samples_5ms); // Remains 5ms

        let samples_12ms =
            source.sample_rate() as usize / 1000 * 12 * source.num_channels() as usize;

        let audio_frame = AudioFrame {
            data: vec![0; samples_12ms].into(),
            sample_rate: source.sample_rate(),
            num_channels: source.num_channels(),
            samples_per_channel: samples_12ms as u32 / source.num_channels(),
        };

        assert!(source.next_frame(&mut inner, &audio_frame).is_some());
        assert!(source.next_frame(&mut inner, &audio_frame).is_none());

        let samples_7ms = source.sample_rate() as usize / 1000 * 7 * source.num_channels() as usize;
        assert_eq!(inner.len, samples_7ms); // Remains 7ms
    }

    #[tokio::test]
    async fn verify_duration() {
        let source = NativeAudioSource::new(AudioSourceOptions::default(), 48000, 2);
        let samples_30ms =
            source.sample_rate() as usize / 1000 * 35 * source.num_channels() as usize;

        let audio_frame = AudioFrame {
            data: vec![0; samples_30ms].into(),
            sample_rate: source.sample_rate(),
            num_channels: source.num_channels(),
            samples_per_channel: samples_30ms as u32 / source.num_channels(),
        };

        let mut inner = source.inner.lock().await;

        let samples_10ms = source.sample_rate() as usize / 100 * source.num_channels() as usize;
        assert_eq!(source.next_frame(&mut inner, &audio_frame).unwrap().len(), samples_10ms);
        assert_eq!(source.next_frame(&mut inner, &audio_frame).unwrap().len(), samples_10ms);
        assert_eq!(source.next_frame(&mut inner, &audio_frame).unwrap().len(), samples_10ms);
        assert!(source.next_frame(&mut inner, &audio_frame).is_none());

        let samples_5ms = source.sample_rate() as usize / 1000 * 5 * source.num_channels() as usize;
        assert_eq!(inner.len, samples_5ms); // Remaining data
    }
}
