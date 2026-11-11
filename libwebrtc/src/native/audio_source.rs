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

use std::{future::Future, sync::Arc};

use cxx::SharedPtr;
use parking_lot::Mutex;
use tokio::sync::oneshot;
use webrtc_sys::audio_track as sys_at;

use crate::{audio_frame::AudioFrame, audio_source::AudioSourceOptions, RtcError, RtcErrorType};

#[derive(Clone)]
pub struct NativeAudioSource {
    sys_handle: SharedPtr<sys_at::ffi::AudioTrackSource>,
    sample_rate: u32,
    num_channels: u32,
    queue_size_samples: u32,
    /// How many times the buffer was cleared. Held while a chunk is queued, so a clear lands
    /// either before the chunk (which is then dropped) or after it (which then clears it).
    clears: Arc<Mutex<u64>>,
}

impl NativeAudioSource {
    /// Creates a new [`NativeAudioSource`].
    ///
    /// # Arguments
    /// * `options` – Configuration options for the source (e.g. echo cancellation, noise suppression).
    /// * `sample_rate` – Sampling rate in Hz (for example, `48000`).
    /// * `num_channels` – Number of audio channels (`1` for mono, `2` for stereo, etc.).
    /// * `queue_size_ms` – Size of the internal buffering queue, in milliseconds.
    ///
    /// # Behavior
    /// - If `queue_size_ms` is **zero**, buffering is **disabled** and audio frames are
    ///   delivered directly to webrtc sinks. In this mode, the caller **must provide 10 ms frames**
    ///   (i.e., `sample_rate / 100` samples per channel) when calling [`capture_frame`].
    /// - If `queue_size_ms` is **non-zero**, buffering is enabled. The value must be a
    ///   **multiple of 10**, representing the total buffering duration in milliseconds.
    ///   Frames will be queued and flushed to sinks asynchronously once the buffer
    ///   reaches the configured threshold.
    ///
    /// # Panics
    /// assert if `queue_size_ms` is not a multiple of 10.
    pub fn new(
        options: AudioSourceOptions,
        sample_rate: u32,
        num_channels: u32,
        queue_size_ms: u32,
    ) -> NativeAudioSource {
        assert!(queue_size_ms % 10 == 0, "queue_size_ms must be a multiple of 10");

        let sys_handle = sys_at::ffi::new_audio_track_source(
            options.into(),
            sample_rate.try_into().unwrap(),
            num_channels.try_into().unwrap(),
            queue_size_ms.try_into().unwrap(),
        );

        let queue_size_samples = (queue_size_ms * sample_rate * num_channels) / 1000;
        Self {
            sys_handle,
            sample_rate,
            num_channels,
            queue_size_samples,
            clears: Arc::new(Mutex::new(0)),
        }
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

    /// Discards the queued audio, and the rest of every capture made before this call.
    pub fn clear_buffer(&self) {
        let mut clears = self.clears.lock();
        *clears += 1;
        self.sys_handle.clear_buffer();
    }

    /// Queues `frame`, resolving once the queue has room for more.
    ///
    /// A frame longer than the queue is fed in queue-sized chunks. A [`clear_buffer`] called
    /// after this method (even before the returned future is first polled) drops the chunks not
    /// queued yet, and the future resolves.
    ///
    /// [`clear_buffer`]: Self::clear_buffer
    pub fn capture_frame(
        &self,
        frame: &AudioFrame<'_>,
    ) -> impl Future<Output = Result<(), RtcError>> + Send + 'static {
        let cleared = *self.clears.lock();
        let checked = self.check_frame(frame);
        let source = self.clone();
        let data = frame.data.to_vec();
        async move {
            checked?;
            if source.queue_size_samples == 0 {
                return source.capture_direct(&data);
            }
            source.capture_queued(cleared, &data).await
        }
    }

    fn check_frame(&self, frame: &AudioFrame<'_>) -> Result<(), RtcError> {
        if self.sample_rate != frame.sample_rate || self.num_channels != frame.num_channels {
            return Err(RtcError {
                error_type: RtcErrorType::InvalidState,
                message: "sample_rate and num_channels don't match".to_owned(),
            });
        }
        if self.queue_size_samples != 0 {
            return Ok(());
        }
        // frame size must be 10ms for fast path
        let expected_frames_per_ch = (self.sample_rate / 100) as usize;
        if frame.data.len() % (self.num_channels as usize) != 0 {
            return Err(RtcError {
                error_type: RtcErrorType::InvalidState,
                message: "frame.data length not divisible by channel count".to_owned(),
            });
        }
        let nb_frames = frame.data.len() / (self.num_channels as usize);
        if nb_frames != expected_frames_per_ch {
            return Err(RtcError {
                error_type: RtcErrorType::InvalidState,
                message: format!(
                    "direct capture requires 10ms frames: got {} frames, expected {}",
                    nb_frames, expected_frames_per_ch
                ),
            });
        }
        Ok(())
    }

    /// Fast path: no buffering.
    fn capture_direct(&self, data: &[i16]) -> Result<(), RtcError> {
        // Define a no-op callback for fast path (queue_size_ms=0)
        // This is safer than passing null, which can cause UB in release mode optimizations
        extern "C" fn noop_complete_callback(_ctx: *const sys_at::SourceContext) {
            // No-op: fast path completes synchronously, no callback needed
        }

        let nb_frames = data.len() / (self.num_channels as usize);
        unsafe {
            // Use a valid no-op callback instead of null for safety
            // In release mode, transmuting null pointers can cause UB
            let noop_callback = sys_at::CompleteCallback(noop_complete_callback);
            let ok = self.sys_handle.capture_frame(
                data,
                self.sample_rate,
                self.num_channels,
                nb_frames,
                std::ptr::null(), // Context is still null - callback won't use it
                noop_callback,
            );
            if !ok {
                return Err(RtcError {
                    error_type: RtcErrorType::InvalidState,
                    message: "failed to capture frame without buffering".to_owned(),
                });
            }
        }
        Ok(())
    }

    /// Buffered path: one queue-sized chunk at a time, until a clear after `cleared`.
    async fn capture_queued(&self, cleared: u64, data: &[i16]) -> Result<(), RtcError> {
        extern "C" fn lk_audio_source_complete(userdata: *const sys_at::SourceContext) {
            let tx = unsafe { Box::from_raw(userdata as *mut oneshot::Sender<()>) };
            let _ = tx.send(());
        }

        for chunk in data.chunks(self.queue_size_samples as usize) {
            let nb_frames = chunk.len() / self.num_channels as usize;
            let (tx, rx) = oneshot::channel::<()>();
            {
                let clears = self.clears.lock();
                if *clears != cleared {
                    return Ok(());
                }
                let ctx_ptr = Box::into_raw(Box::new(tx)) as *const sys_at::SourceContext;
                unsafe {
                    // In the fast path, C++ never store / invoke on_complete / ctx.
                    if !self.sys_handle.capture_frame(
                        chunk,
                        self.sample_rate,
                        self.num_channels,
                        nb_frames,
                        ctx_ptr,
                        sys_at::CompleteCallback(lk_audio_source_complete),
                    ) {
                        drop(Box::from_raw(ctx_ptr as *mut oneshot::Sender<()>));
                        return Err(RtcError {
                            error_type: RtcErrorType::InvalidState,
                            message: "failed to capture frame".to_owned(),
                        });
                    }
                }
            }
            // A clear empties the queue, so this resolves within the source's next 10ms tick.
            let _ = rx.await;
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
