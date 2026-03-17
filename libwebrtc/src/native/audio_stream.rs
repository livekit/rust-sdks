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
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};

use cxx::SharedPtr;
use livekit_runtime::Stream;
use parking_lot::Mutex;
use rtrb::{Consumer, Producer, PushError, RingBuffer};
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
    kind: AudioFrameQueueKind,
    closed: AtomicBool,
    dropped_frames: AtomicU64,
    waker: Mutex<Option<Waker>>,
}

enum AudioFrameQueueKind {
    Bounded(BoundedAudioFrameQueue),
    Unbounded(UnboundedAudioFrameQueue),
}

struct BoundedAudioFrameQueue {
    producer: Mutex<Producer<AudioFrame<'static>>>,
    consumer: Mutex<Consumer<AudioFrame<'static>>>,
}

struct UnboundedAudioFrameQueue {
    frames: Mutex<VecDeque<AudioFrame<'static>>>,
}

impl AudioFrameQueue {
    fn new(capacity: Option<usize>) -> Self {
        let kind = match capacity.filter(|capacity| *capacity > 0) {
            Some(capacity) => {
                let (producer, consumer) = RingBuffer::new(capacity);
                AudioFrameQueueKind::Bounded(BoundedAudioFrameQueue {
                    producer: Mutex::new(producer),
                    consumer: Mutex::new(consumer),
                })
            }
            None => AudioFrameQueueKind::Unbounded(UnboundedAudioFrameQueue {
                frames: Mutex::new(VecDeque::new()),
            }),
        };

        Self {
            kind,
            closed: AtomicBool::new(false),
            dropped_frames: AtomicU64::new(0),
            waker: Mutex::new(None),
        }
    }

    fn push(&self, frame: AudioFrame<'static>) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }

        match &self.kind {
            AudioFrameQueueKind::Bounded(queue) => self.push_bounded(queue, frame),
            AudioFrameQueueKind::Unbounded(queue) => {
                queue.frames.lock().push_back(frame);
            }
        }

        self.wake_receiver();
    }

    fn push_bounded(&self, queue: &BoundedAudioFrameQueue, mut frame: AudioFrame<'static>) {
        loop {
            let push_result = queue.producer.lock().push(frame);
            match push_result {
                Ok(()) => return,
                Err(PushError::Full(returned_frame)) => {
                    frame = returned_frame;

                    let dropped = queue.consumer.lock().pop().is_ok();

                    if dropped {
                        self.record_drop();
                    } else {
                        return;
                    }
                }
            }
        }
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.wake_receiver();

        match &self.kind {
            AudioFrameQueueKind::Bounded(queue) => {
                let mut consumer = queue.consumer.lock();
                while consumer.pop().is_ok() {}
            }
            AudioFrameQueueKind::Unbounded(queue) => {
                queue.frames.lock().clear();
            }
        }
    }

    fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<AudioFrame<'static>>> {
        if let Some(frame) = self.try_pop() {
            return Poll::Ready(Some(frame));
        }

        if self.closed.load(Ordering::Acquire) {
            return Poll::Ready(None);
        }

        *self.waker.lock() = Some(cx.waker().clone());

        if let Some(frame) = self.try_pop() {
            self.waker.lock().take();
            Poll::Ready(Some(frame))
        } else if self.closed.load(Ordering::Acquire) {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }

    fn try_pop(&self) -> Option<AudioFrame<'static>> {
        match &self.kind {
            AudioFrameQueueKind::Bounded(queue) => queue.consumer.lock().pop().ok(),
            AudioFrameQueueKind::Unbounded(queue) => queue.frames.lock().pop_front(),
        }
    }

    fn wake_receiver(&self) {
        let waker = self.waker.lock().take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    fn record_drop(&self) {
        let dropped_frames = self.dropped_frames.fetch_add(1, Ordering::Relaxed) + 1;
        if dropped_frames == 1 || dropped_frames % 100 == 0 {
            log::warn!(
                "native audio stream queue overflow; dropped {} queued frames",
                dropped_frames
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::AudioFrameQueue;
    use crate::audio_frame::AudioFrame;

    fn test_frame(marker: i16) -> AudioFrame<'static> {
        AudioFrame {
            data: vec![marker].into(),
            sample_rate: 48_000,
            num_channels: 1,
            samples_per_channel: 1,
        }
    }

    fn pop_marker(queue: &AudioFrameQueue) -> Option<i16> {
        queue.try_pop().map(|frame| frame.data[0])
    }

    #[test]
    fn bounded_queue_preserves_fifo_order_under_capacity() {
        let queue = AudioFrameQueue::new(Some(3));

        queue.push(test_frame(1));
        queue.push(test_frame(2));
        queue.push(test_frame(3));

        assert_eq!(pop_marker(&queue), Some(1));
        assert_eq!(pop_marker(&queue), Some(2));
        assert_eq!(pop_marker(&queue), Some(3));
        assert_eq!(pop_marker(&queue), None);
    }

    #[test]
    fn bounded_queue_drops_oldest_when_full() {
        let queue = AudioFrameQueue::new(Some(2));

        queue.push(test_frame(1));
        queue.push(test_frame(2));
        queue.push(test_frame(3));

        assert_eq!(queue.dropped_frames.load(Ordering::Relaxed), 1);
        assert_eq!(pop_marker(&queue), Some(2));
        assert_eq!(pop_marker(&queue), Some(3));
        assert_eq!(pop_marker(&queue), None);
    }

    #[test]
    fn unbounded_queue_retains_all_frames() {
        let queue = AudioFrameQueue::new(None);

        for marker in 1..=4 {
            queue.push(test_frame(marker));
        }

        for marker in 1..=4 {
            assert_eq!(pop_marker(&queue), Some(marker));
        }
        assert_eq!(pop_marker(&queue), None);
        assert_eq!(queue.dropped_frames.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn close_clears_buffer_and_rejects_future_pushes() {
        let queue = AudioFrameQueue::new(Some(2));

        queue.push(test_frame(1));
        queue.close();
        queue.push(test_frame(2));

        assert_eq!(pop_marker(&queue), None);
    }
}
