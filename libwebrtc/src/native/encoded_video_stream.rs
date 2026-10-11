// Copyright 2026 LiveKit, Inc.
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

//! Stream of encoded access units from a remote video track whose factory uses
//! pass-through decoding.

use std::{
    collections::VecDeque,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};

use cxx::{SharedPtr, UniquePtr};
use parking_lot::Mutex;
use tokio_stream::Stream;
use webrtc_sys::video_track as sys_vt;

use super::video_frame::new_video_frame_buffer;
use crate::{
    video_frame::{ntp_time_from_raw, BoxVideoBuffer, EncodedFrameType, EncodedVideoCodec},
    video_track::RtcVideoTrack,
};

/// Options for [`NativeEncodedVideoStream`].
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct NativeEncodedVideoStreamOptions {
    /// Maximum number of access units waiting to be read.
    ///
    /// `None` (or `Some(0)`) never discards a frame. With `Some(n)`, a reader that falls `n`
    /// frames behind loses the whole backlog at once rather than single frames:
    /// the queue is flushed, a keyframe is requested, and frames are marked
    /// [`ReceivedEncodedFrame::after_gap`] until it arrives. Dropping single
    /// frames instead would leave later frames referencing ones the reader
    /// never saw.
    pub max_pending_frames: Option<usize>,
}

/// One encoded access unit received through a pass-through decoder.
pub struct ReceivedEncodedFrame {
    /// Codec of the access unit.
    pub codec: EncodedVideoCodec,
    /// Whether the access unit is a keyframe.
    pub frame_type: EncodedFrameType,
    /// Frame size from the most recent keyframe.
    pub width: u32,
    /// Frame size from the most recent keyframe.
    pub height: u32,
    /// RTP timestamp (90 kHz).
    pub rtp_timestamp: u32,
    /// Capture time in the receiver's NTP clock, once RTCP sender reports or
    /// the absolute-capture-time extension make it known.
    pub ntp_time_ms: Option<i64>,
    /// libwebrtc's timestamp for the frame, in microseconds.
    pub timestamp_us: i64,
    /// Delivery order from the current decoder, starting at 0.
    pub sequence: u64,
    /// `true` when an earlier access unit this one may depend on was not
    /// delivered (a gap, a new decoder, or a flushed backlog) and no keyframe
    /// has arrived since. Such frames may not decode correctly; a keyframe has
    /// already been requested.
    pub after_gap: bool,
    buffer: BoxVideoBuffer,
}

impl std::fmt::Debug for ReceivedEncodedFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReceivedEncodedFrame")
            .field("codec", &self.codec)
            .field("frame_type", &self.frame_type)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("rtp_timestamp", &self.rtp_timestamp)
            .field("sequence", &self.sequence)
            .field("after_gap", &self.after_gap)
            .field("payload_len", &self.payload().len())
            .finish()
    }
}

impl ReceivedEncodedFrame {
    /// The access unit, as received (no copy).
    pub fn payload(&self) -> &[u8] {
        self.buffer.as_native().and_then(|native| native.encoded()).map_or(&[], |e| e.payload)
    }

    /// Asks the sender for a keyframe, e.g. after the application failed to
    /// decode this frame.
    pub fn request_keyframe(&self) {
        if let Some(native) = self.buffer.as_native() {
            native.request_keyframe();
        }
    }
}

/// A [`Stream`] of the access units received on a remote video track, for
/// factories created with
/// [`VideoDecoderBackend::PassThrough`](crate::peer_connection_factory::VideoDecoderBackend::PassThrough).
///
/// Unlike `NativeVideoStream`, it never drops single frames (see
/// [`NativeEncodedVideoStreamOptions::max_pending_frames`]), and it tracks
/// delivery order so that frames after a gap are flagged and a keyframe is
/// requested once per gap.
pub struct NativeEncodedVideoStream {
    native_sink: SharedPtr<sys_vt::ffi::NativeVideoSink>,
    queue: Arc<EncodedFrameQueue<ReceivedEncodedFrame>>,
    video_track: RtcVideoTrack,
}

impl NativeEncodedVideoStream {
    /// Starts receiving the encoded frames of `video_track`.
    pub fn new(video_track: RtcVideoTrack, options: NativeEncodedVideoStreamOptions) -> Self {
        let queue = Arc::new(EncodedFrameQueue::new(options.max_pending_frames));
        let observer = Arc::new(EncodedTrackObserver {
            queue: Arc::downgrade(&queue),
            gaps: Mutex::new(GapTracker::default()),
        });
        let native_sink =
            sys_vt::ffi::new_native_video_sink(Box::new(sys_vt::VideoSinkWrapper::new(observer)));
        let video = unsafe { sys_vt::ffi::media_to_video(video_track.sys_handle()) };
        video.add_sink(&native_sink);
        Self { native_sink, queue, video_track }
    }

    /// The track this stream reads.
    pub fn track(&self) -> RtcVideoTrack {
        self.video_track.clone()
    }

    /// Stops receiving; frames still queued are discarded.
    pub fn close(&mut self) {
        let video = unsafe { sys_vt::ffi::media_to_video(self.video_track.sys_handle()) };
        video.remove_sink(&self.native_sink);
        self.queue.close();
    }
}

impl Drop for NativeEncodedVideoStream {
    fn drop(&mut self) {
        self.close();
    }
}

impl Stream for NativeEncodedVideoStream {
    type Item = ReceivedEncodedFrame;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.queue.poll_recv(cx)
    }
}

struct EncodedTrackObserver {
    // Weak: the sink (owned by libwebrtc's track) holds this observer, and the
    // stream owns the queue; the observer must not keep the queue alive.
    queue: std::sync::Weak<EncodedFrameQueue<ReceivedEncodedFrame>>,
    gaps: Mutex<GapTracker>,
}

impl sys_vt::VideoSink for EncodedTrackObserver {
    fn on_frame(&self, frame: UniquePtr<webrtc_sys::video_frame::ffi::VideoFrame>) {
        let Some(queue) = self.queue.upgrade() else {
            return;
        };
        let buffer = new_video_frame_buffer(unsafe { frame.video_frame_buffer() });
        let Some(encoded) = buffer.as_native().and_then(|native| native.encoded()) else {
            log::warn!("NativeEncodedVideoStream received a decoded frame; is the factory using VideoDecoderBackend::PassThrough?");
            return;
        };
        let (codec, frame_type, stream_id, sequence) =
            (encoded.codec, encoded.frame_type, encoded.stream_id, encoded.sequence);

        let mut gaps = self.gaps.lock();
        let flushed = queue.take_overflow_flag();
        let verdict =
            gaps.observe(stream_id, sequence, frame_type == EncodedFrameType::Key, flushed);
        let received = ReceivedEncodedFrame {
            codec,
            frame_type,
            width: buffer.width(),
            height: buffer.height(),
            rtp_timestamp: frame.timestamp(),
            ntp_time_ms: ntp_time_from_raw(frame.ntp_time_ms()),
            timestamp_us: frame.timestamp_us(),
            sequence,
            after_gap: verdict.after_gap,
            buffer,
        };
        if verdict.request_keyframe {
            received.request_keyframe();
        }
        drop(gaps);
        queue.push(received);
    }

    fn on_discarded_frame(&self) {}

    fn on_constraints_changed(&self, _constraints: sys_vt::ffi::VideoTrackSourceConstraints) {}
}

/// What [`GapTracker::observe`] decided for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GapVerdict {
    after_gap: bool,
    request_keyframe: bool,
}

/// Tracks the reference chain of received frames.
///
/// The chain is broken by a missing sequence number, a new decoder (a new
/// stream id), a flushed backlog, or a stream that starts on a delta frame. A
/// keyframe restores it. A keyframe is requested once per break.
#[derive(Debug, Default)]
struct GapTracker {
    last: Option<(usize, u64)>,
    broken: bool,
}

impl GapTracker {
    fn observe(
        &mut self,
        stream_id: usize,
        sequence: u64,
        keyframe: bool,
        flushed: bool,
    ) -> GapVerdict {
        let continuous = match self.last {
            Some((id, prev)) if id == stream_id => sequence == prev.wrapping_add(1),
            // A new decoder starts a new chain; only a keyframe can open it.
            _ => false,
        };
        self.last = Some((stream_id, sequence));
        if keyframe {
            self.broken = false;
            return GapVerdict { after_gap: false, request_keyframe: false };
        }
        let breaks = !continuous || flushed;
        let request_keyframe = breaks && !self.broken;
        self.broken |= breaks;
        GapVerdict { after_gap: self.broken, request_keyframe }
    }
}

/// Unbounded or flush-on-overflow FIFO feeding [`NativeEncodedVideoStream`].
struct EncodedFrameQueue<T> {
    frames: Mutex<VecDeque<T>>,
    max_pending: Option<usize>,
    overflowed: AtomicBool,
    closed: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl<T> EncodedFrameQueue<T> {
    fn new(max_pending: Option<usize>) -> Self {
        Self {
            frames: Mutex::new(VecDeque::new()),
            max_pending: max_pending.filter(|n| *n > 0),
            overflowed: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            waker: Mutex::new(None),
        }
    }

    fn push(&self, item: T) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let mut frames = self.frames.lock();
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        if self.max_pending.is_some_and(|n| frames.len() >= n) {
            frames.clear();
            self.overflowed.store(true, Ordering::Release);
        }
        frames.push_back(item);
        drop(frames);
        if let Some(waker) = self.waker.lock().take() {
            waker.wake();
        }
    }

    /// Returns and clears whether the backlog was flushed since the last call.
    fn take_overflow_flag(&self) -> bool {
        self.overflowed.swap(false, Ordering::AcqRel)
    }

    fn poll_recv(&self, cx: &mut Context) -> Poll<Option<T>> {
        let mut frames = self.frames.lock();
        if let Some(item) = frames.pop_front() {
            return Poll::Ready(Some(item));
        }
        if self.closed.load(Ordering::Acquire) {
            return Poll::Ready(None);
        }
        // Registered while holding the frame lock, so a push cannot slip in
        // between the empty check and the waker registration unnoticed.
        *self.waker.lock() = Some(cx.waker().clone());
        Poll::Pending
    }

    fn close(&self) {
        let mut frames = self.frames.lock();
        self.closed.store(true, Ordering::Release);
        frames.clear();
        drop(frames);
        if let Some(waker) = self.waker.lock().take() {
            waker.wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::task::{Context, Poll, Waker};

    use super::{EncodedFrameQueue, GapTracker, GapVerdict};

    fn drain<T>(queue: &EncodedFrameQueue<T>) -> Vec<T> {
        let mut cx = Context::from_waker(Waker::noop());
        let mut out = Vec::new();
        while let Poll::Ready(Some(item)) = queue.poll_recv(&mut cx) {
            out.push(item);
        }
        out
    }

    const OK: GapVerdict = GapVerdict { after_gap: false, request_keyframe: false };
    const GAP_REQUEST: GapVerdict = GapVerdict { after_gap: true, request_keyframe: true };
    const GAP_HELD: GapVerdict = GapVerdict { after_gap: true, request_keyframe: false };

    #[test]
    fn encoded_queue_never_drops() {
        let queue = EncodedFrameQueue::new(None);
        for i in 0..10_000u64 {
            queue.push(i);
        }
        assert_eq!(drain(&queue), (0..10_000).collect::<Vec<_>>());
        assert!(!queue.take_overflow_flag());
    }

    #[test]
    fn overflow_flushes_backlog_and_flags_it() {
        let queue = EncodedFrameQueue::new(Some(4));
        for i in 0..5u64 {
            queue.push(i);
        }
        // The fifth push found 4 pending: the backlog is flushed and the new
        // frame kept, never a single frame dropped from the middle.
        assert!(queue.take_overflow_flag());
        assert_eq!(drain(&queue), [4]);
        assert!(!queue.take_overflow_flag());
    }

    #[test]
    fn close_clears_buffer_and_rejects_future_pushes() {
        let queue = EncodedFrameQueue::new(None);
        queue.push(1u64);
        queue.close();
        queue.push(2u64);
        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(queue.poll_recv(&mut cx), Poll::Ready(None)));
    }

    #[test]
    fn gap_marks_frames_until_keyframe_and_requests_once() {
        let mut t = GapTracker::default();
        let s = 7;
        assert_eq!(t.observe(s, 0, true, false), OK);
        assert_eq!(t.observe(s, 1, false, false), OK);
        assert_eq!(t.observe(s, 3, false, false), GAP_REQUEST); // 2 missing
        assert_eq!(t.observe(s, 4, false, false), GAP_HELD);
        assert_eq!(t.observe(s, 5, true, false), OK);
        assert_eq!(t.observe(s, 6, false, false), OK);
    }

    #[test]
    fn stream_waits_for_first_keyframe() {
        let mut t = GapTracker::default();
        assert_eq!(t.observe(1, 0, false, false), GAP_REQUEST);
        assert_eq!(t.observe(1, 1, false, false), GAP_HELD);
        assert_eq!(t.observe(1, 2, true, false), OK);
    }

    #[test]
    fn new_stream_identity_restarts_sequence_without_false_gap() {
        let mut t = GapTracker::default();
        assert_eq!(t.observe(1, 0, true, false), OK);
        assert_eq!(t.observe(1, 1, false, false), OK);
        // A new decoder (codec switch) starts at 0 with a keyframe: no gap.
        assert_eq!(t.observe(2, 0, true, false), OK);
        assert_eq!(t.observe(2, 1, false, false), OK);
        // A new decoder whose first frame is a delta cannot be decoded.
        assert_eq!(t.observe(3, 0, false, false), GAP_REQUEST);
        assert_eq!(t.observe(3, 1, true, false), OK);
    }

    #[test]
    fn flushed_backlog_breaks_the_chain() {
        let mut t = GapTracker::default();
        assert_eq!(t.observe(1, 0, true, false), OK);
        assert_eq!(t.observe(1, 1, false, true), GAP_REQUEST);
        assert_eq!(t.observe(1, 2, false, false), GAP_HELD);
        assert_eq!(t.observe(1, 3, true, false), OK);
    }
}
