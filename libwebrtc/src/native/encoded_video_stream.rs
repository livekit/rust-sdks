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
    /// Lifecycle tests check that the sink and its observer are released.
    #[cfg(test)]
    observer: std::sync::Weak<EncodedTrackObserver>,
}

impl NativeEncodedVideoStream {
    /// Starts receiving the encoded frames of `video_track`.
    pub fn new(video_track: RtcVideoTrack, options: NativeEncodedVideoStreamOptions) -> Self {
        let queue = Arc::new(EncodedFrameQueue::new(options.max_pending_frames));
        let observer = Arc::new(EncodedTrackObserver {
            queue: Arc::downgrade(&queue),
            gaps: Mutex::new(GapTracker::default()),
        });
        #[cfg(test)]
        let weak_observer = Arc::downgrade(&observer);
        let native_sink =
            sys_vt::ffi::new_native_video_sink(Box::new(sys_vt::VideoSinkWrapper::new(observer)));
        let video = unsafe { sys_vt::ffi::media_to_video(video_track.sys_handle()) };
        video.add_sink(&native_sink);
        Self {
            native_sink,
            queue,
            video_track,
            #[cfg(test)]
            observer: weak_observer,
        }
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

/// T3: a pre-encoded sender and a pass-through receiver in one process,
/// connected over host candidates.
#[cfg(test)]
mod loopback_tests {
    use std::time::Duration;

    use tokio::{
        sync::mpsc,
        time::{timeout, Instant, MissedTickBehavior},
    };
    use tokio_stream::StreamExt;

    use super::{NativeEncodedVideoStream, NativeEncodedVideoStreamOptions, ReceivedEncodedFrame};
    use crate::{
        media_stream_track::MediaStreamTrack,
        peer_connection::{AnswerOptions, OfferOptions, PeerConnection, TrackEvent},
        peer_connection_factory::{
            native::PeerConnectionFactoryExt, ContinualGatheringPolicy, PeerConnectionFactory,
            RtcConfiguration, VideoDecoderBackend,
        },
        rtp_sender::VideoEncoderBackend,
        rtp_transceiver::{RtpTransceiverDirection, RtpTransceiverInit},
        video_frame::{
            EncodedFrameType, EncodedVideoCodec, EncodedVideoFrame, I420Buffer, VideoFrame,
            VideoRotation,
        },
        video_source::{native::NativeVideoSource, VideoResolution},
        video_track::RtcVideoTrack,
        MediaType,
    };

    include!("../../../webrtc-sys/test/fixtures/passthrough_streams.rs");

    const FRAME_INTERVAL: Duration = Duration::from_micros(33_333);
    const RECEIVE_TIMEOUT: Duration = Duration::from_secs(5);
    /// 90 kHz RTP clock at 30 fps.
    const RTP_TICKS_PER_FRAME: i64 = 3000;

    fn codec_of(stream: &TestStream) -> EncodedVideoCodec {
        match stream.codec {
            "H264" => EncodedVideoCodec::H264,
            "H265" => EncodedVideoCodec::H265,
            "VP8" => EncodedVideoCodec::VP8,
            "VP9" => EncodedVideoCodec::VP9,
            "AV1" => EncodedVideoCodec::AV1,
            other => panic!("unknown fixture codec {other}"),
        }
    }

    type Unit = (bool, &'static [u8]);

    /// One access unit as sent, for comparison with what arrives.
    #[derive(Clone, Copy)]
    struct Sent {
        stream: &'static TestStream,
        unit: Unit,
    }

    #[derive(Default)]
    struct LoopbackConfig {
        /// The receiver's `RtcConfiguration::prerenderer_smoothing`.
        prerenderer_smoothing: Option<bool>,
        options: NativeEncodedVideoStreamOptions,
        /// The receiving peer connection also sends (raw) video back.
        receiver_sends: bool,
    }

    struct Loopback {
        source: NativeVideoSource,
        /// The receiving side's raw source, with `receiver_sends`.
        back_source: Option<NativeVideoSource>,
        stream: NativeEncodedVideoStream,
        remote_track: RtcVideoTrack,
        sender_pc: PeerConnection,
        receiver_pc: PeerConnection,
        factories: Option<(PeerConnectionFactory, PeerConnectionFactory)>,
    }

    impl Loopback {
        /// Connects a pre-encoded sender to a pass-through receiver for the
        /// codec of `stream`. Returns `None` when this build cannot negotiate
        /// the codec: libwebrtc only offers pass-through formats for codecs a
        /// real encoder supports, so H265 needs a hardware encoder.
        async fn new(stream: &'static TestStream, config: LoopbackConfig) -> Option<Self> {
            let LoopbackConfig { prerenderer_smoothing, options, receiver_sends } = config;
            let _ = env_logger::builder().is_test(true).try_init();
            let sender_factory = PeerConnectionFactory::default();
            let receiver_factory =
                PeerConnectionFactory::with_video_decoder_backend(VideoDecoderBackend::PassThrough);

            let mime = format!("video/{}", stream.codec);
            let mut codecs: Vec<_> = sender_factory
                .get_rtp_sender_capabilities(MediaType::Video)
                .codecs
                .into_iter()
                .filter(|c| c.mime_type.eq_ignore_ascii_case(&mime))
                .collect();
            if codecs.is_empty() {
                eprintln!("skipping: {mime} cannot be negotiated without a real encoder");
                return None;
            }
            // The fixtures are Constrained Baseline, packetization mode 1.
            codecs.sort_by_key(|c| {
                !c.sdp_fmtp_line.as_deref().is_some_and(|f| {
                    f.contains("profile-level-id=42e01f") && f.contains("packetization-mode=1")
                })
            });

            let config = |prerenderer_smoothing| RtcConfiguration {
                continual_gathering_policy: ContinualGatheringPolicy::GatherOnce,
                prerenderer_smoothing,
                ..Default::default()
            };
            let sender_pc = sender_factory.create_peer_connection(config(None)).unwrap();
            let receiver_pc =
                receiver_factory.create_peer_connection(config(prerenderer_smoothing)).unwrap();

            let source = NativeVideoSource::new_encoded(VideoResolution {
                width: stream.width,
                height: stream.height,
            });
            let track = sender_factory.create_video_track("video", source.clone());
            let transceiver = sender_pc
                .add_transceiver(
                    MediaStreamTrack::Video(track),
                    RtpTransceiverInit {
                        direction: if receiver_sends {
                            RtpTransceiverDirection::SendRecv
                        } else {
                            RtpTransceiverDirection::SendOnly
                        },
                        stream_ids: vec!["stream".into()],
                        send_encodings: vec![],
                    },
                )
                .unwrap();
            transceiver.sender().set_video_encoder_backend(VideoEncoderBackend::PreEncoded);
            transceiver.set_codec_preferences(codecs).unwrap();

            let back_source = receiver_sends.then(|| {
                let back_source = NativeVideoSource::new(
                    VideoResolution { width: stream.width, height: stream.height },
                    false,
                );
                let back = receiver_factory.create_video_track("back", back_source.clone());
                receiver_pc.add_track(MediaStreamTrack::Video(back), &["back"]).unwrap();
                back_source
            });

            let (track_tx, mut track_rx) = mpsc::unbounded_channel();
            receiver_pc.on_track(Some(Box::new(move |event: TrackEvent| {
                let _ = track_tx.send(event.track);
            })));
            connect(&sender_pc, &receiver_pc).await;
            let remote_track = match timeout(RECEIVE_TIMEOUT, track_rx.recv()).await {
                Ok(Some(MediaStreamTrack::Video(track))) => track,
                other => panic!("no remote video track: {other:?}"),
            };
            let stream = NativeEncodedVideoStream::new(remote_track.clone(), options);

            Some(Self {
                source,
                back_source,
                stream,
                remote_track,
                sender_pc,
                receiver_pc,
                factories: Some((sender_factory, receiver_factory)),
            })
        }

        fn send(&self, sent: Sent) {
            if let Some(back_source) = &self.back_source {
                back_source.capture_frame(&VideoFrame::new(
                    VideoRotation::VideoRotation0,
                    I420Buffer::new(sent.stream.width, sent.stream.height),
                ));
            }
            let (is_key, payload) = sent.unit;
            self.source.capture_encoded_frame(&EncodedVideoFrame {
                codec: codec_of(sent.stream),
                payload,
                timestamp_us: 0,
                frame_type: EncodedFrameType::from_keyframe(is_key),
                resolution: VideoResolution {
                    width: sent.stream.width,
                    height: sent.stream.height,
                },
                frame_metadata: None,
            });
        }

        /// Sends keyframes until one arrives, then waits until nothing more is
        /// in flight, so that tests start from a connected, idle loopback.
        async fn start(&mut self, stream: &'static TestStream) {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                self.send(Sent { stream, unit: stream.units[0] });
                match timeout(Duration::from_millis(100), self.stream.next()).await {
                    Ok(Some(_)) => break,
                    Ok(None) => panic!("stream ended during warm-up"),
                    Err(_) => assert!(Instant::now() < deadline, "no frame within 10 s"),
                }
            }
            while let Ok(Some(_)) = timeout(Duration::from_millis(300), self.stream.next()).await {}
            while self.source.take_keyframe_request() {}
        }

        async fn receive(&mut self, count: usize) -> Vec<ReceivedEncodedFrame> {
            let mut frames = Vec::with_capacity(count);
            while frames.len() < count {
                match timeout(RECEIVE_TIMEOUT, self.stream.next()).await {
                    Ok(Some(frame)) => frames.push(frame),
                    Ok(None) => panic!("stream ended after {} of {count} frames", frames.len()),
                    Err(_) => panic!("only {} of {count} frames arrived", frames.len()),
                }
            }
            frames
        }

        /// Frames that arrive within `wait` of each other.
        async fn receive_until_idle(&mut self, wait: Duration) -> Vec<ReceivedEncodedFrame> {
            let mut frames = Vec::new();
            while let Ok(Some(frame)) = timeout(wait, self.stream.next()).await {
                frames.push(frame);
            }
            frames
        }
    }

    impl Drop for Loopback {
        fn drop(&mut self) {
            // The ICE forwarding tasks hold the peer connections; clearing the
            // callbacks ends them.
            for pc in [&self.sender_pc, &self.receiver_pc] {
                pc.on_ice_candidate(None);
                pc.on_track(None);
                pc.close();
            }
        }
    }

    async fn connect(offerer: &PeerConnection, answerer: &PeerConnection) {
        let (offerer_tx, mut offerer_rx) = mpsc::unbounded_channel();
        let (answerer_tx, mut answerer_rx) = mpsc::unbounded_channel();
        offerer.on_ice_candidate(Some(Box::new(move |c| {
            let _ = offerer_tx.send(c);
        })));
        answerer.on_ice_candidate(Some(Box::new(move |c| {
            let _ = answerer_tx.send(c);
        })));

        // `offer_to_receive_video: false` would strip the receive direction
        // from every video transceiver (`sdp_offer_answer.cc`).
        let options = OfferOptions { offer_to_receive_video: true, ..Default::default() };
        let offer = offerer.create_offer(options).await.unwrap();
        offerer.set_local_description(offer.clone()).await.unwrap();
        answerer.set_remote_description(offer).await.unwrap();
        let answer = answerer.create_answer(AnswerOptions::default()).await.unwrap();
        answerer.set_local_description(answer.clone()).await.unwrap();
        offerer.set_remote_description(answer).await.unwrap();

        let (o, a) = (offerer.clone(), answerer.clone());
        tokio::spawn(async move {
            while let Some(c) = offerer_rx.recv().await {
                let _ = a.add_ice_candidate(c).await;
            }
        });
        tokio::spawn(async move {
            while let Some(c) = answerer_rx.recv().await {
                let _ = o.add_ice_candidate(c).await;
            }
        });
    }

    fn cycle(stream: &'static TestStream, count: usize) -> Vec<Sent> {
        stream.units.iter().cycle().take(count).map(|&unit| Sent { stream, unit }).collect()
    }

    async fn send_paced(lb: &Loopback, sent: &[Sent], interval: Duration) {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        for &s in sent {
            ticker.tick().await;
            lb.send(s);
        }
    }

    fn assert_contiguous(frames: &[ReceivedEncodedFrame]) {
        for pair in frames.windows(2) {
            assert_eq!(pair[1].sequence, pair[0].sequence + 1, "{:?} -> {:?}", pair[0], pair[1]);
        }
        assert!(frames.iter().all(|f| !f.after_gap), "{frames:?}");
    }

    fn assert_same_units(sent: &[Sent], received: &[ReceivedEncodedFrame]) {
        assert_eq!(received.len(), sent.len());
        for (i, (s, r)) in sent.iter().zip(received).enumerate() {
            assert_eq!(r.codec, codec_of(s.stream), "frame {i}");
            assert_eq!(r.frame_type, EncodedFrameType::from_keyframe(s.unit.0), "frame {i}");
            assert_eq!((r.width, r.height), (s.stream.width, s.stream.height), "frame {i}");
            assert!(r.payload() == s.unit.1, "frame {i}: payload differs");
        }
    }

    // T3 #25: every codec arrives byte-exact, in order, with its RTP timing.
    async fn roundtrip(stream: &'static TestStream) {
        let Some(mut lb) = Loopback::new(stream, Default::default()).await else {
            return;
        };
        lb.start(stream).await;
        let sent = cycle(stream, 3 * stream.units.len());
        send_paced(&lb, &sent, FRAME_INTERVAL).await;
        let received = lb.receive(sent.len()).await;

        assert_same_units(&sent, &received);
        assert_contiguous(&received);
        // Capture times are aligned to the sender's clock and rounded to
        // milliseconds before becoming RTP timestamps, so single deltas jitter;
        // their mean is the frame interval.
        let deltas: Vec<i64> = received
            .windows(2)
            .map(|p| p[1].rtp_timestamp.wrapping_sub(p[0].rtp_timestamp) as i32 as i64)
            .collect();
        assert!(deltas.iter().all(|d| *d > 0 && *d < 2 * RTP_TICKS_PER_FRAME), "{deltas:?}");
        let mean = deltas.iter().sum::<i64>() / deltas.len() as i64;
        assert!((mean - RTP_TICKS_PER_FRAME).abs() < RTP_TICKS_PER_FRAME / 10, "{deltas:?}");
    }

    #[tokio::test]
    async fn roundtrip_h264() {
        roundtrip(&H264).await;
    }

    #[tokio::test]
    async fn roundtrip_h265() {
        roundtrip(&H265).await;
    }

    #[tokio::test]
    async fn roundtrip_vp8() {
        roundtrip(&VP8).await;
    }

    #[tokio::test]
    async fn roundtrip_vp9() {
        roundtrip(&VP9).await;
    }

    #[tokio::test]
    async fn roundtrip_av1() {
        roundtrip(&AV1).await;
    }

    fn nal_units(access_unit: &[u8]) -> Vec<&[u8]> {
        let mut nals = Vec::new();
        let mut rest = access_unit;
        while let Some(start) = rest.windows(4).position(|w| w == [0, 0, 0, 1]) {
            rest = &rest[start + 4..];
            let end = rest.windows(4).position(|w| w == [0, 0, 0, 1]).unwrap_or(rest.len());
            nals.push(&rest[..end]);
        }
        nals
    }

    /// T3 #26: an SPS without the VUI bitstream restriction is rewritten by
    /// libwebrtc (`SpsVuiRewriter`); every other NAL unit arrives unchanged.
    #[tokio::test]
    async fn h264_sps_without_vui_restriction_is_rewritten_only_in_sps() {
        const SPS: u8 = 7;
        let stream = &H264_NO_VUI;
        let mut lb = Loopback::new(stream, Default::default()).await.unwrap();
        lb.start(stream).await;
        let sent = cycle(stream, stream.units.len());
        send_paced(&lb, &sent, FRAME_INTERVAL).await;
        let received = lb.receive(sent.len()).await;

        for (i, (s, r)) in sent.iter().zip(&received).enumerate() {
            assert_eq!((r.width, r.height), (stream.width, stream.height), "frame {i}");
            let (sent_nals, received_nals) = (nal_units(s.unit.1), nal_units(r.payload()));
            assert_eq!(sent_nals.len(), received_nals.len(), "frame {i}");
            for (sn, rn) in sent_nals.iter().zip(&received_nals) {
                if sn[0] & 0x1F == SPS {
                    assert_ne!(sn, rn, "frame {i}: SPS was not rewritten");
                } else {
                    assert_eq!(sn, rn, "frame {i}: NAL type {} differs", sn[0] & 0x1F);
                }
            }
        }
        assert!(received.iter().any(|r| r.frame_type == EncodedFrameType::Key));
    }

    const BURSTS: usize = 5;
    const BURST_LEN: usize = 30;
    /// Fast enough that frames reach the receiver far faster than their
    /// capture rate, slow enough that the sender does not drop them: libwebrtc
    /// drops a captured frame when the next one is posted before the encoder
    /// queue picked it up, or when two share a capture millisecond
    /// (`frame_cadence_adapter.cc`, `video_stream_encoder.cc`).
    const BURST_INTERVAL: Duration = Duration::from_millis(5);

    async fn send_bursts(
        lb: &mut Loopback,
        stream: &'static TestStream,
    ) -> Vec<ReceivedEncodedFrame> {
        let mut received = Vec::new();
        for _ in 0..BURSTS {
            send_paced(lb, &cycle(stream, BURST_LEN), BURST_INTERVAL).await;
            received.extend(lb.receive_until_idle(Duration::from_secs(1)).await);
        }
        received
    }

    async fn frames_sent(lb: &Loopback) -> usize {
        use crate::stats::RtcStats;
        lb.sender_pc
            .get_stats()
            .await
            .unwrap()
            .into_iter()
            .find_map(|stats| match stats {
                RtcStats::OutboundRtp(o) => Some(o.outbound.frames_sent as usize),
                _ => None,
            })
            .unwrap_or(0)
    }

    /// Where frames went: `(encoded, sent)` on the sender and `(received,
    /// decoded, dropped)` on the receiver, from libwebrtc's statistics.
    async fn frame_counters(lb: &Loopback) -> String {
        use crate::stats::RtcStats;
        let mut out = String::new();
        for stats in lb.sender_pc.get_stats().await.unwrap() {
            if let RtcStats::OutboundRtp(o) = stats {
                out += &format!(
                    "sender encoded {} sent {}; ",
                    o.outbound.frames_encoded, o.outbound.frames_sent
                );
            }
        }
        for stats in lb.receiver_pc.get_stats().await.unwrap() {
            if let RtcStats::InboundRtp(i) = stats {
                out += &format!(
                    "receiver received {} decoded {} dropped {}",
                    i.inbound.frames_received, i.inbound.frames_decoded, i.inbound.frames_dropped
                );
            }
        }
        out
    }

    /// T3 #27: frames that arrive in bursts all reach the stream.
    #[tokio::test]
    async fn burst_delivery_loses_nothing() {
        let mut lb = Loopback::new(&VP8, Default::default()).await.unwrap();
        lb.start(&VP8).await;
        let before = frames_sent(&lb).await;
        let received = send_bursts(&mut lb, &VP8).await;
        let sent = frames_sent(&lb).await - before;
        // Under CPU load the sender itself may drop a capture (V21) and hold
        // the following deltas until a keyframe; that is not this test's
        // subject. Everything the sender sent must arrive.
        assert_eq!(received.len(), sent, "{}", frame_counters(&lb).await);
        assert!(sent * 10 >= BURSTS * BURST_LEN * 9, "{}", frame_counters(&lb).await);
        assert_contiguous(&received);
    }

    /// T3 #29: a reader that stops polling loses nothing.
    #[tokio::test]
    async fn slow_consumer_loses_nothing() {
        let mut lb = Loopback::new(&VP8, Default::default()).await.unwrap();
        lb.start(&VP8).await;
        let sent = cycle(&VP8, 60);
        send_paced(&lb, &sent, FRAME_INTERVAL).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let received = lb.receive(sent.len()).await;
        assert_same_units(&sent, &received);
        assert_contiguous(&received);
    }

    /// Sends deltas until the sender sees a keyframe request, answers it with a
    /// keyframe, and returns the frames received meanwhile. `request` runs once,
    /// on the first received frame.
    async fn keyframe_request_roundtrip(
        lb: &mut Loopback,
        request: impl FnOnce(&Loopback, &ReceivedEncodedFrame),
    ) {
        let stream = &VP8;
        let deltas = &stream.units[1..];
        let mut request = Some(request);
        let mut ticker = tokio::time::interval(FRAME_INTERVAL);
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut requested = false;
        let mut i = 0;
        while !requested {
            assert!(Instant::now() < deadline, "the sender saw no keyframe request within 2 s");
            ticker.tick().await;
            lb.send(Sent { stream, unit: deltas[i % deltas.len()] });
            i += 1;
            if let Ok(Some(frame)) = timeout(Duration::from_millis(1), lb.stream.next()).await {
                if let Some(request) = request.take() {
                    request(lb, &frame);
                }
            }
            requested = lb.source.take_keyframe_request();
        }
        // On the next tick: a capture in the same millisecond as the last one
        // would be dropped by the sender.
        ticker.tick().await;
        lb.send(Sent { stream, unit: stream.units[0] });
        let deadline = Instant::now() + RECEIVE_TIMEOUT;
        loop {
            let frame = timeout(deadline - Instant::now(), lb.stream.next())
                .await
                .expect("the requested keyframe did not arrive")
                .unwrap();
            if frame.frame_type == EncodedFrameType::Key {
                break;
            }
        }
    }

    /// T3 #30: `RtcVideoTrack::request_keyframe` reaches the sender.
    #[tokio::test]
    async fn receiver_request_keyframe_reaches_sender() {
        let mut lb = Loopback::new(&VP8, Default::default()).await.unwrap();
        lb.start(&VP8).await;
        keyframe_request_roundtrip(&mut lb, |lb, _| lb.remote_track.request_keyframe()).await;
    }

    /// T3 #31: a request raised on a received frame reaches the sender through
    /// the decoder (`WEBRTC_VIDEO_CODEC_OK_REQUEST_KEYFRAME`).
    #[tokio::test]
    async fn decoder_flag_request_reaches_sender() {
        let mut lb = Loopback::new(&VP8, Default::default()).await.unwrap();
        lb.start(&VP8).await;
        keyframe_request_roundtrip(&mut lb, |_, frame| frame.request_keyframe()).await;
    }

    /// T3 #32: a keyframe at a new size switches the reported size without a
    /// new decoder or a gap.
    #[tokio::test]
    async fn resolution_change_mid_stream() {
        let mut lb = Loopback::new(&H264, Default::default()).await.unwrap();
        lb.start(&H264).await;
        let mut sent = cycle(&H264, H264.units.len());
        sent.extend(cycle(&H264_128X72, H264_128X72.units.len()));
        send_paced(&lb, &sent, FRAME_INTERVAL).await;
        let received = lb.receive(sent.len()).await;
        assert_same_units(&sent, &received);
        assert_contiguous(&received);
    }

    /// Sends VP8 at 30 fps for `duration` and returns the received frames.
    async fn stream_for(lb: &mut Loopback, duration: Duration) -> Vec<ReceivedEncodedFrame> {
        let count = (duration.as_millis() / FRAME_INTERVAL.as_millis()) as usize;
        let mut ticker = tokio::time::interval(FRAME_INTERVAL);
        let mut received = Vec::new();
        for sent in cycle(&VP8, count) {
            ticker.tick().await;
            lb.send(sent);
            while let Ok(Some(frame)) = timeout(Duration::ZERO, lb.stream.next()).await {
                received.push(frame);
            }
        }
        received
    }

    /// T3 #33: the capture time in the receiver's NTP clock needs an RTT on
    /// the receive stream (`rtp_video_stream_receiver2.cc`). A receive-only
    /// peer connection gets none unless RTCP XR RRTR or abs-capture-time is
    /// negotiated, and neither is by default.
    #[tokio::test]
    async fn ntp_time_unknown_on_receive_only_connection() {
        let mut lb = Loopback::new(&VP8, Default::default()).await.unwrap();
        lb.start(&VP8).await;
        let received = stream_for(&mut lb, Duration::from_secs(3)).await;
        assert!(!received.is_empty());
        assert!(received.iter().all(|f| f.ntp_time_ms.is_none()), "{received:?}");
    }

    /// T3 #33b: when the receiving peer connection also sends video, the RTT
    /// comes from the sender's reports and the NTP time becomes known.
    #[tokio::test]
    async fn ntp_time_becomes_available_when_receiver_also_sends() {
        let config = LoopbackConfig { receiver_sends: true, ..Default::default() };
        let mut lb = Loopback::new(&VP8, config).await.unwrap();
        lb.start(&VP8).await;
        let received = stream_for(&mut lb, Duration::from_secs(5)).await;
        assert!(received.iter().any(|f| f.ntp_time_ms.is_some()), "no NTP time within 5 s");
    }

    /// T3 #34: factories without pass-through decoding still decode.
    #[tokio::test]
    async fn default_factory_still_decodes() {
        use crate::video_stream::native::NativeVideoStream;

        let _ = env_logger::builder().is_test(true).try_init();
        let (sender_factory, receiver_factory) =
            (PeerConnectionFactory::default(), PeerConnectionFactory::default());
        let config = RtcConfiguration {
            continual_gathering_policy: ContinualGatheringPolicy::GatherOnce,
            ..Default::default()
        };
        let sender_pc = sender_factory.create_peer_connection(config.clone()).unwrap();
        let receiver_pc = receiver_factory.create_peer_connection(config).unwrap();
        let source = NativeVideoSource::new(VideoResolution { width: 64, height: 64 }, false);
        let track = sender_factory.create_video_track("video", source.clone());
        sender_pc.add_track(MediaStreamTrack::Video(track), &["stream"]).unwrap();
        let (track_tx, mut track_rx) = mpsc::unbounded_channel();
        receiver_pc.on_track(Some(Box::new(move |event: TrackEvent| {
            let _ = track_tx.send(event.track);
        })));
        connect(&sender_pc, &receiver_pc).await;
        let Ok(Some(MediaStreamTrack::Video(remote))) =
            timeout(RECEIVE_TIMEOUT, track_rx.recv()).await
        else {
            panic!("no remote video track");
        };
        let mut stream = NativeVideoStream::new(remote);

        let mut ticker = tokio::time::interval(FRAME_INTERVAL);
        let deadline = Instant::now() + Duration::from_secs(10);
        let frame = loop {
            assert!(Instant::now() < deadline, "no decoded frame within 10 s");
            ticker.tick().await;
            source.capture_frame(&VideoFrame::new(
                VideoRotation::VideoRotation0,
                I420Buffer::new(64, 64),
            ));
            if let Ok(Some(frame)) = timeout(Duration::from_millis(1), stream.next()).await {
                break frame;
            }
        };
        assert_eq!((frame.buffer.width(), frame.buffer.height()), (64, 64));
        assert!(frame.buffer.as_native().and_then(|n| n.encoded()).is_none());
        assert!(frame.buffer.to_i420().data().0.len() >= 64 * 64);

        for pc in [&sender_pc, &receiver_pc] {
            pc.on_ice_candidate(None);
            pc.on_track(None);
            pc.close();
        }
    }

    // T4: lifecycle (AGENTS.md "Memory lifecycle").

    /// Weak handles to everything a stream owns or registers.
    struct StreamProbes {
        observer: std::sync::Weak<super::EncodedTrackObserver>,
        queue: std::sync::Weak<super::EncodedFrameQueue<ReceivedEncodedFrame>>,
    }

    impl StreamProbes {
        fn of(stream: &NativeEncodedVideoStream) -> Self {
            Self {
                observer: stream.observer.clone(),
                queue: std::sync::Arc::downgrade(&stream.queue),
            }
        }

        fn assert_released(&self) {
            assert!(self.observer.upgrade().is_none(), "the sink observer is still alive");
            assert!(self.queue.upgrade().is_none(), "the frame queue is still alive");
        }
    }

    /// T4 #35: dropping the stream unregisters its sink. The observer is owned
    /// by the native sink, which the track keeps until `remove_sink`, so the
    /// observer's release proves the unregistration.
    #[tokio::test]
    async fn encoded_stream_drop_unregisters_sink() {
        let mut lb = Loopback::new(&VP8, Default::default()).await.unwrap();
        lb.start(&VP8).await;
        let replacement =
            NativeEncodedVideoStream::new(lb.remote_track.clone(), Default::default());
        let stream = std::mem::replace(&mut lb.stream, replacement);
        let probes = StreamProbes::of(&stream);
        drop(stream);
        probes.assert_released();

        // The track still delivers to the other stream.
        send_paced(&lb, &cycle(&VP8, VP8.units.len()), FRAME_INTERVAL).await;
        assert_eq!(lb.receive(VP8.units.len()).await.len(), VP8.units.len());
    }

    #[cfg(target_os = "linux")]
    fn open_fds_and_threads() -> (usize, usize) {
        let count = |dir| std::fs::read_dir(dir).unwrap().count();
        (count("/proc/self/fd"), count("/proc/self/task"))
    }

    /// One complete session: connect, deliver five frames, tear down.
    async fn one_session() -> StreamProbes {
        let mut lb = Loopback::new(&VP8, Default::default()).await.unwrap();
        lb.start(&VP8).await;
        send_paced(&lb, &cycle(&VP8, VP8.units.len()), FRAME_INTERVAL).await;
        lb.receive(VP8.units.len()).await;
        let probes = StreamProbes::of(&lb.stream);
        drop(lb);
        probes
    }

    /// T4 #36: repeated sessions release everything they create.
    #[tokio::test]
    async fn repeated_session_workload() {
        const SESSIONS: usize = 20;
        // The first session creates process-wide state (threads, sockets)
        // that later sessions reuse; measure from after it.
        one_session().await.assert_released();
        tokio::time::sleep(Duration::from_millis(500)).await;
        #[cfg(target_os = "linux")]
        let baseline = open_fds_and_threads();

        let mut probes = Vec::new();
        for _ in 0..SESSIONS {
            probes.push(one_session().await);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        probes.iter().for_each(StreamProbes::assert_released);

        #[cfg(target_os = "linux")]
        {
            let (fds, threads) = open_fds_and_threads();
            // A small tolerance for libwebrtc's lazily started helpers.
            assert!(fds <= baseline.0 + 4, "fds: {} -> {fds}", baseline.0);
            assert!(threads <= baseline.1 + 4, "threads: {} -> {threads}", baseline.1);
        }
    }

    /// T4 #37: asking for a keyframe after the connection closed is harmless,
    /// through the track and through a frame received earlier.
    #[tokio::test]
    async fn request_keyframe_after_close_is_noop() {
        let mut lb = Loopback::new(&VP8, Default::default()).await.unwrap();
        lb.start(&VP8).await;
        send_paced(&lb, &cycle(&VP8, 1), FRAME_INTERVAL).await;
        let frame = lb.receive(1).await.pop().unwrap();
        let track = lb.remote_track.clone();
        lb.receiver_pc.close();
        track.request_keyframe();
        frame.request_keyframe();

        let probes = StreamProbes::of(&lb.stream);
        drop(lb);
        track.request_keyframe();
        frame.request_keyframe();
        drop(frame);
        drop(track);
        probes.assert_released();
    }

    /// T4 #38: the factories can go before the stream (partial teardown).
    #[tokio::test]
    async fn factory_dropped_with_live_stream() {
        let mut lb = Loopback::new(&VP8, Default::default()).await.unwrap();
        lb.start(&VP8).await;
        drop(lb.factories.take());

        // The peer connections keep the native factory alive: frames still flow.
        send_paced(&lb, &cycle(&VP8, VP8.units.len()), FRAME_INTERVAL).await;
        assert_eq!(lb.receive(VP8.units.len()).await.len(), VP8.units.len());

        let replacement =
            NativeEncodedVideoStream::new(lb.remote_track.clone(), Default::default());
        let stream = std::mem::replace(&mut lb.stream, replacement);
        let probes = StreamProbes::of(&stream);
        drop(stream);
        probes.assert_released();
        drop(lb);
    }
}
