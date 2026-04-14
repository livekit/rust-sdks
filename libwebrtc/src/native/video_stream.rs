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

use cxx::{SharedPtr, UniquePtr};
use livekit_runtime::Stream;
use parking_lot::Mutex;
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use webrtc_sys::video_track as sys_vt;

use super::video_frame::new_video_frame_buffer;
use crate::{
    native::packet_trailer::PacketTrailerHandler,
    video_frame::{BoxVideoFrame, FrameMetadata, VideoFrame},
    video_track::RtcVideoTrack,
};

pub struct NativeVideoStream {
    native_sink: SharedPtr<sys_vt::ffi::NativeVideoSink>,
    observer: Arc<VideoTrackObserver>,
    video_track: RtcVideoTrack,
    frame_queue: Arc<VideoFrameQueue>,
}

impl NativeVideoStream {
    pub fn new(video_track: RtcVideoTrack, queue_size_frames: Option<usize>) -> Self {
        let frame_queue = Arc::new(VideoFrameQueue::new(queue_size_frames));
        // Auto-wire the packet trailer handler from the track if one is set.
        let handler = video_track.handle.packet_trailer_handler();
        let observer = Arc::new(VideoTrackObserver {
            frame_queue: frame_queue.clone(),
            packet_trailer_handler: parking_lot::Mutex::new(handler),
        });
        let native_sink = sys_vt::ffi::new_native_video_sink(Box::new(
            sys_vt::VideoSinkWrapper::new(observer.clone()),
        ));

        let video = unsafe { sys_vt::ffi::media_to_video(video_track.sys_handle()) };
        video.add_sink(&native_sink);

        Self { native_sink, observer, video_track, frame_queue }
    }

    /// Set the packet trailer handler for this stream.
    ///
    /// When set, each frame produced by this stream will have its
    /// `user_timestamp` field populated from the handler's receive
    /// map (looked up by RTP timestamp).
    ///
    /// Note: If the handler was already set on the `RtcVideoTrack` before
    /// creating this stream, it is automatically wired up. This method is
    /// only needed if you want to override or set the handler after
    /// construction.
    pub fn set_packet_trailer_handler(&self, handler: PacketTrailerHandler) {
        *self.observer.packet_trailer_handler.lock() = Some(handler);
    }

    pub fn track(&self) -> RtcVideoTrack {
        self.video_track.clone()
    }

    pub fn close(&mut self) {
        let video = unsafe { sys_vt::ffi::media_to_video(self.video_track.sys_handle()) };
        video.remove_sink(&self.native_sink);
        self.frame_queue.close();
    }
}

impl Drop for NativeVideoStream {
    fn drop(&mut self) {
        self.close();
    }
}

impl Stream for NativeVideoStream {
    type Item = BoxVideoFrame;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.frame_queue.poll_recv(cx)
    }
}

struct VideoTrackObserver {
    frame_queue: Arc<VideoFrameQueue>,
    packet_trailer_handler: parking_lot::Mutex<Option<PacketTrailerHandler>>,
}

impl sys_vt::VideoSink for VideoTrackObserver {
    fn on_frame(&self, frame: UniquePtr<webrtc_sys::video_frame::ffi::VideoFrame>) {
        let rtp_timestamp = frame.timestamp();
        let frame_metadata = self
            .packet_trailer_handler
            .lock()
            .as_ref()
            .and_then(|h| h.lookup_frame_metadata(rtp_timestamp))
            .map(|(ts, fid)| FrameMetadata {
                user_timestamp: Some(ts),
                frame_id: if fid != 0 { Some(fid) } else { None },
            });

        self.frame_queue.push(VideoFrame {
            rotation: frame.rotation().into(),
            timestamp_us: frame.timestamp_us(),
            frame_metadata,
            buffer: new_video_frame_buffer(unsafe { frame.video_frame_buffer() }),
        });
    }

    fn on_discarded_frame(&self) {}

    fn on_constraints_changed(&self, _constraints: sys_vt::ffi::VideoTrackSourceConstraints) {}
}

struct VideoFrameQueue {
    kind: VideoFrameQueueKind,
    closed: AtomicBool,
    dropped_frames: AtomicU64,
    waker: Mutex<Option<Waker>>,
}

enum VideoFrameQueueKind {
    Bounded(BoundedVideoFrameQueue),
    Unbounded(UnboundedVideoFrameQueue),
}

struct BoundedVideoFrameQueue {
    producer: Mutex<Producer<BoxVideoFrame>>,
    consumer: Mutex<Consumer<BoxVideoFrame>>,
}

struct UnboundedVideoFrameQueue {
    frames: Mutex<VecDeque<BoxVideoFrame>>,
}

impl VideoFrameQueue {
    fn new(capacity: Option<usize>) -> Self {
        let kind = match capacity.filter(|capacity| *capacity > 0) {
            Some(capacity) => {
                let (producer, consumer) = RingBuffer::new(capacity);
                VideoFrameQueueKind::Bounded(BoundedVideoFrameQueue {
                    producer: Mutex::new(producer),
                    consumer: Mutex::new(consumer),
                })
            }
            None => VideoFrameQueueKind::Unbounded(UnboundedVideoFrameQueue {
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

    fn push(&self, frame: BoxVideoFrame) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }

        match &self.kind {
            VideoFrameQueueKind::Bounded(queue) => self.push_bounded(queue, frame),
            VideoFrameQueueKind::Unbounded(queue) => {
                queue.frames.lock().push_back(frame);
            }
        }

        self.wake_receiver();
    }

    fn push_bounded(&self, queue: &BoundedVideoFrameQueue, mut frame: BoxVideoFrame) {
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
            VideoFrameQueueKind::Bounded(queue) => {
                let mut consumer = queue.consumer.lock();
                while consumer.pop().is_ok() {}
            }
            VideoFrameQueueKind::Unbounded(queue) => {
                queue.frames.lock().clear();
            }
        }
    }

    fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<BoxVideoFrame>> {
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

    fn try_pop(&self) -> Option<BoxVideoFrame> {
        match &self.kind {
            VideoFrameQueueKind::Bounded(queue) => queue.consumer.lock().pop().ok(),
            VideoFrameQueueKind::Unbounded(queue) => queue.frames.lock().pop_front(),
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
                "native video stream queue overflow; dropped {} queued frames",
                dropped_frames
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::VideoFrameQueue;
    use crate::video_frame::{BoxVideoFrame, I420Buffer, VideoFrame, VideoRotation};

    fn test_frame(timestamp_us: i64) -> BoxVideoFrame {
        VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us,
            buffer: Box::new(I420Buffer::new(2, 2)),
        }
    }

    fn pop_timestamp(queue: &VideoFrameQueue) -> Option<i64> {
        queue.try_pop().map(|frame| frame.timestamp_us)
    }

    #[test]
    fn bounded_queue_preserves_fifo_order_under_capacity() {
        let queue = VideoFrameQueue::new(Some(3));

        queue.push(test_frame(1));
        queue.push(test_frame(2));
        queue.push(test_frame(3));

        assert_eq!(pop_timestamp(&queue), Some(1));
        assert_eq!(pop_timestamp(&queue), Some(2));
        assert_eq!(pop_timestamp(&queue), Some(3));
        assert_eq!(pop_timestamp(&queue), None);
    }

    #[test]
    fn bounded_queue_drops_oldest_when_full() {
        let queue = VideoFrameQueue::new(Some(2));

        queue.push(test_frame(1));
        queue.push(test_frame(2));
        queue.push(test_frame(3));

        assert_eq!(queue.dropped_frames.load(Ordering::Relaxed), 1);
        assert_eq!(pop_timestamp(&queue), Some(2));
        assert_eq!(pop_timestamp(&queue), Some(3));
        assert_eq!(pop_timestamp(&queue), None);
    }

    #[test]
    fn unbounded_queue_retains_all_frames() {
        let queue = VideoFrameQueue::new(None);

        for timestamp_us in 1..=4 {
            queue.push(test_frame(timestamp_us));
        }

        for timestamp_us in 1..=4 {
            assert_eq!(pop_timestamp(&queue), Some(timestamp_us));
        }
        assert_eq!(pop_timestamp(&queue), None);
        assert_eq!(queue.dropped_frames.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn close_clears_buffer_and_rejects_future_pushes() {
        let queue = VideoFrameQueue::new(Some(2));

        queue.push(test_frame(1));
        queue.close();
        queue.push(test_frame(2));

        assert_eq!(pop_timestamp(&queue), None);
    }
}
