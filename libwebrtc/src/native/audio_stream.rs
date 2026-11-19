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
use parking_lot::Mutex;
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use tokio_stream::Stream;
use webrtc_sys::audio_track as sys_at;

use crate::{audio_frame::AudioFrame, audio_track::RtcAudioTrack};

pub struct NativeAudioStream {
    native_sink: SharedPtr<sys_at::ffi::NativeAudioSink>,
    audio_track: RtcAudioTrack,
    frame_queue: Arc<AudioFrameQueue>,
    observer: Arc<AudioTrackObserver>,
}

impl NativeAudioStream {
    pub fn new(
        audio_track: RtcAudioTrack,
        sample_rate: i32,
        num_channels: i32,
        queue_size_frames: Option<usize>,
    ) -> Self {
        Self::new_with_frame_size(audio_track, sample_rate, num_channels, queue_size_frames, None)
    }

    /// Creates a sink with optional aggregation for an unbounded queue.
    pub fn new_with_frame_size(
        audio_track: RtcAudioTrack,
        sample_rate: i32,
        num_channels: i32,
        queue_size_frames: Option<usize>,
        frame_size_ms: Option<u32>,
    ) -> Self {
        let frame_queue = Arc::new(AudioFrameQueue::new(queue_size_frames));
        // Bounded queues count original sink frames; keep their overflow semantics unchanged.
        let target = frame_size_ms
            .filter(|ms| *ms > 10 && queue_size_frames.is_none())
            .map(|ms| sample_rate as usize * ms as usize / 1000 * num_channels as usize);
        let observer = Arc::new(AudioTrackObserver {
            frame_queue: frame_queue.clone(),
            batcher: target.map(|samples| Mutex::new(FrameBatcher::new(samples))),
        });
        let native_sink = sys_at::ffi::new_native_audio_sink(
            Box::new(sys_at::AudioSinkWrapper::new(observer.clone())),
            sample_rate,
            num_channels,
        );

        let audio = unsafe { sys_at::ffi::media_to_audio(audio_track.sys_handle()) };
        audio.add_sink(&native_sink);

        Self { native_sink, audio_track, frame_queue, observer }
    }

    pub fn track(&self) -> RtcAudioTrack {
        self.audio_track.clone()
    }

    pub fn close(&mut self) {
        let audio = unsafe { sys_at::ffi::media_to_audio(self.audio_track.sys_handle()) };
        audio.remove_sink(&self.native_sink);

        self.frame_queue.close();
        if let Some(batcher) = &self.observer.batcher {
            batcher.lock().pending.clear();
        }
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
    batcher: Option<Mutex<FrameBatcher>>,
}

impl sys_at::AudioSink for AudioTrackObserver {
    fn on_data(&self, data: &[i16], sample_rate: i32, nb_channels: usize, nb_frames: usize) {
        if let Some(batcher) = &self.batcher {
            batcher.lock().push(data, |data| {
                self.frame_queue.push(AudioFrame {
                    samples_per_channel: (data.len() / nb_channels) as u32,
                    data: data.into(),
                    sample_rate: sample_rate as u32,
                    num_channels: nb_channels as u32,
                })
            });
            return;
        }
        self.frame_queue.push(AudioFrame {
            data: data.to_owned().into(),
            sample_rate: sample_rate as u32,
            num_channels: nb_channels as u32,
            samples_per_channel: nb_frames as u32,
        });
    }
}

/// Accumulates PCM before waking the async consumer, without changing sink cadence.
struct FrameBatcher {
    pending: Vec<i16>,
    target: usize,
}

impl FrameBatcher {
    /// Allocates the accumulation buffer for one output frame.
    fn new(target: usize) -> Self {
        Self { pending: Vec::with_capacity(target), target }
    }

    /// Emits complete frames while retaining an incomplete tail.
    fn push(&mut self, mut data: &[i16], mut emit: impl FnMut(Vec<i16>)) {
        while !data.is_empty() {
            let copied = data.len().min(self.target - self.pending.len());
            self.pending.extend_from_slice(&data[..copied]);
            data = &data[copied..];
            if self.pending.len() == self.target {
                emit(std::mem::replace(&mut self.pending, Vec::with_capacity(self.target)));
            }
        }
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

#[cfg(test)]
mod batching_tests {
    use super::*;

    #[test]
    fn arbitrary_chunks_preserve_samples_and_tail() {
        for target in [56, 80, 160, 312, 1200, 4800] {
            let input: Vec<i16> = (0..15000).map(|i| i as i16).collect();
            for chunk in [1, 80, 240, 1023, 4096] {
                let mut batcher = FrameBatcher::new(target);
                let mut output = Vec::new();
                for part in input.chunks(chunk) {
                    batcher.push(part, |frame| {
                        assert_eq!(frame.len(), target);
                        output.extend(frame);
                    });
                }
                assert_eq!(output.len(), input.len() / target * target);
                output.extend(batcher.pending);
                assert_eq!(output, input);
            }
        }
    }

    #[test]
    fn no_early_delivery_and_observer_releases_tail() {
        let queue = Arc::new(AudioFrameQueue::new(None));
        let observer = Arc::new(AudioTrackObserver {
            frame_queue: queue.clone(),
            batcher: Some(Mutex::new(FrameBatcher::new(1200))),
        });
        use webrtc_sys::audio_track::AudioSink;
        observer.on_data(&[1; 240], 24000, 1, 240);
        let mut cx = Context::from_waker(std::task::Waker::noop());
        assert!(queue.poll_recv(&mut cx).is_pending());
        let weak = Arc::downgrade(&observer);
        drop(observer);
        assert!(weak.upgrade().is_none());
        queue.close();
        assert!(matches!(queue.poll_recv(&mut cx), Poll::Ready(None)));
    }
}
