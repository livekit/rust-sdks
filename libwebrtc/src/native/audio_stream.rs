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

use std::{
    collections::VecDeque,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use cxx::SharedPtr;
use livekit_runtime::Stream;
use webrtc_sys::audio_track as sys_at;

use crate::{audio_frame::AudioFrame, audio_track::RtcAudioTrack};

pub struct NativeAudioStream {
    native_sink: SharedPtr<sys_at::ffi::NativeAudioSink>,
    audio_track: RtcAudioTrack,
    frame_queue: Arc<AudioFrameQueue>,
}

impl NativeAudioStream {
    pub fn new(
        audio_track: RtcAudioTrack,
        sample_rate: i32,
        num_channels: i32,
        queue_size_frames: Option<usize>,
    ) -> Self {
        let frame_queue = Arc::new(AudioFrameQueue::new(queue_size_frames));
        let observer = Arc::new(AudioTrackObserver { frame_queue: frame_queue.clone() });
        let native_sink = sys_at::ffi::new_native_audio_sink(
            Box::new(sys_at::AudioSinkWrapper::new(observer.clone())),
            sample_rate,
            num_channels,
        );

        let audio = unsafe { sys_at::ffi::media_to_audio(audio_track.sys_handle()) };
        audio.add_sink(&native_sink);

        Self { native_sink, audio_track, frame_queue }
    }

    pub fn track(&self) -> RtcAudioTrack {
        self.audio_track.clone()
    }

    pub fn close(&mut self) {
        let audio = unsafe { sys_at::ffi::media_to_audio(self.audio_track.sys_handle()) };
        audio.remove_sink(&self.native_sink);

        self.frame_queue.close();
    }
}

impl Drop for NativeAudioStream {
    fn drop(&mut self) {
        self.close();
    }
}

impl Stream for NativeAudioStream {
    type Item = AudioFrame<'static>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.frame_queue.poll_recv(cx)
    }
}

pub struct AudioTrackObserver {
    frame_queue: Arc<AudioFrameQueue>,
}

impl sys_at::AudioSink for AudioTrackObserver {
    fn on_data(&self, data: &[i16], sample_rate: i32, nb_channels: usize, nb_frames: usize) {
        self.frame_queue.push(AudioFrame {
            data: data.to_owned().into(),
            sample_rate: sample_rate as u32,
            num_channels: nb_channels as u32,
            samples_per_channel: nb_frames as u32,
        });
    }
}

struct AudioFrameQueue {
    state: Mutex<AudioFrameQueueState>,
}

struct AudioFrameQueueState {
    frames: VecDeque<AudioFrame<'static>>,
    capacity: Option<usize>,
    closed: bool,
    waker: Option<std::task::Waker>,
    dropped_frames: u64,
}

impl AudioFrameQueue {
    fn new(capacity: Option<usize>) -> Self {
        Self {
            state: Mutex::new(AudioFrameQueueState {
                frames: VecDeque::new(),
                capacity: capacity.filter(|capacity| *capacity > 0),
                closed: false,
                waker: None,
                dropped_frames: 0,
            }),
        }
    }

    fn push(&self, frame: AudioFrame<'static>) {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return;
        }

        if let Some(capacity) = state.capacity {
            while state.frames.len() >= capacity {
                state.frames.pop_front();
                state.dropped_frames += 1;
                if state.dropped_frames == 1 || state.dropped_frames % 100 == 0 {
                    log::warn!(
                        "native audio stream queue overflow; dropped {} queued frames",
                        state.dropped_frames
                    );
                }
            }
        }

        state.frames.push_back(frame);
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }

    fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        state.frames.clear();
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }

    fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<AudioFrame<'static>>> {
        let mut state = self.state.lock().unwrap();

        if let Some(frame) = state.frames.pop_front() {
            return Poll::Ready(Some(frame));
        }

        if state.closed {
            return Poll::Ready(None);
        }

        state.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}
