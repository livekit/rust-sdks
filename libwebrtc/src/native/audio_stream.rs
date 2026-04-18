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

use crate::{
    audio_frame::{AudioFrame, AudioFrameTimestamp},
    audio_track::RtcAudioTrack,
    native::packet_trailer::PacketTrailerHandler,
};

pub struct NativeAudioStream {
    native_sink: SharedPtr<sys_at::ffi::NativeAudioSink>,
    audio_track: RtcAudioTrack,
    frame_queue: Arc<AudioFrameQueue>,
    packet_trailer_handler: Arc<Mutex<Option<PacketTrailerHandler>>>,
}

impl NativeAudioStream {
    pub fn new(
        audio_track: RtcAudioTrack,
        sample_rate: i32,
        num_channels: i32,
        queue_size_frames: Option<usize>,
    ) -> Self {
        let frame_queue = Arc::new(AudioFrameQueue::new(queue_size_frames));
        let packet_trailer_handler = Arc::new(Mutex::new(audio_track.packet_trailer_handler()));
        let observer = Arc::new(AudioTrackObserver {
            frame_queue: frame_queue.clone(),
            packet_trailer_handler: packet_trailer_handler.clone(),
            anchor: Mutex::new(None),
        });
        let native_sink = sys_at::ffi::new_native_audio_sink(
            Box::new(sys_at::AudioSinkWrapper::new(observer.clone())),
            sample_rate,
            num_channels,
        );

        let audio = unsafe { sys_at::ffi::media_to_audio(audio_track.sys_handle()) };
        audio.add_sink(&native_sink);

        Self { native_sink, audio_track, frame_queue, packet_trailer_handler }
    }

    pub fn track(&self) -> RtcAudioTrack {
        self.audio_track.clone()
    }

    pub fn set_packet_trailer_handler(&self, handler: PacketTrailerHandler) {
        *self.packet_trailer_handler.lock() = Some(handler);
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
    packet_trailer_handler: Arc<Mutex<Option<PacketTrailerHandler>>>,
    anchor: Mutex<Option<AudioTimestampAnchor>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AudioTimestampAnchor {
    // Audio trailers are sparse reference points; decoded frame timing is
    // derived from the latest anchor plus RTP progression at the audio clock rate.
    rtp_timestamp: u32,
    user_timestamp_us: u64,
    frame_id: Option<u32>,
    sample_rate: u32,
}

const AUDIO_ANCHOR_MAX_BEHIND_RTP_TICKS: u32 = 1_920;
const AUDIO_ANCHOR_MAX_AHEAD_RTP_TICKS: u32 = 120;

fn is_newer_or_same_rtp_timestamp(previous: u32, candidate: u32) -> bool {
    let delta = candidate.wrapping_sub(previous);
    delta == 0 || delta < (u32::MAX / 2) + 1
}

fn anchor_is_usable_for_rtp(
    anchor: AudioTimestampAnchor,
    rtp_timestamp: u32,
    max_behind: u32,
) -> bool {
    rtp_delta_signed(anchor.rtp_timestamp, rtp_timestamp)
        .is_some_and(|delta| delta >= 0 && delta as u32 <= max_behind)
}

fn rtp_delta_signed(base: u32, target: u32) -> Option<i64> {
    let delta = target.wrapping_sub(base);
    if delta < (u32::MAX / 2) + 1 {
        Some(delta as i64)
    } else {
        let reverse = base.wrapping_sub(target);
        if reverse < (u32::MAX / 2) + 1 {
            Some(-(reverse as i64))
        } else {
            None
        }
    }
}

fn derive_user_timestamp_us(anchor: AudioTimestampAnchor, rtp_timestamp: u32) -> Option<u64> {
    let rtp_delta = rtp_delta_signed(anchor.rtp_timestamp, rtp_timestamp)?;
    let delta_us = ((rtp_delta as i128) * 1_000_000i128) / anchor.sample_rate as i128;
    let derived = anchor.user_timestamp_us as i128 + delta_us;
    u64::try_from(derived).ok()
}

impl sys_at::AudioSink for AudioTrackObserver {
    fn on_data(
        &self,
        data: &[i16],
        sample_rate: i32,
        nb_channels: usize,
        nb_frames: usize,
        rtp_timestamp: Option<u32>,
    ) {
        let sample_rate = sample_rate as u32;
        let valid_rtp_timestamp = rtp_timestamp.filter(|rtp_timestamp| *rtp_timestamp != 0);
        let timestamp =
            valid_rtp_timestamp.map(|rtp_timestamp| AudioFrameTimestamp { rtp_timestamp });
        let frame_metadata = self
            .packet_trailer_handler
            .lock()
            .as_ref()
            .and_then(|handler| {
                let metadata = match valid_rtp_timestamp {
                    Some(rtp_timestamp) => {
                        if let Some((user_timestamp, frame_id, packet_rtp_timestamp)) = handler.lookup_nearest_audio_metadata(
                            rtp_timestamp,
                            AUDIO_ANCHOR_MAX_BEHIND_RTP_TICKS,
                            AUDIO_ANCHOR_MAX_AHEAD_RTP_TICKS,
                        )
                        {
                            let anchor = AudioTimestampAnchor {
                                rtp_timestamp: packet_rtp_timestamp,
                                user_timestamp_us: user_timestamp,
                                frame_id: (frame_id != 0).then_some(frame_id),
                                sample_rate,
                            };
                            let mut current_anchor = self.anchor.lock();
                            if current_anchor
                                .map(|previous| {
                                    is_newer_or_same_rtp_timestamp(
                                        previous.rtp_timestamp,
                                        anchor.rtp_timestamp,
                                    )
                                })
                                .unwrap_or(true)
                            {
                                *current_anchor = Some(anchor);
                            }
                            derive_user_timestamp_us(anchor, rtp_timestamp).map(|derived_user_timestamp| {
                                crate::video_frame::FrameMetadata {
                                    user_timestamp: Some(derived_user_timestamp),
                                    frame_id: anchor.frame_id,
                                }
                            })
                        } else if let Some(anchor) = *self.anchor.lock() {
                            if !anchor_is_usable_for_rtp(
                                anchor,
                                rtp_timestamp,
                                AUDIO_ANCHOR_MAX_BEHIND_RTP_TICKS,
                            ) {
                                *self.anchor.lock() = None;
                                None
                            } else {
                                derive_user_timestamp_us(anchor, rtp_timestamp).map(
                                    |derived_user_timestamp| crate::video_frame::FrameMetadata {
                                        user_timestamp: Some(derived_user_timestamp),
                                        frame_id: None,
                                    },
                                )
                            }
                        } else {
                            None
                        }
                    }
                    None => {
                        let fallback = handler.pop_next_received_metadata();
                        fallback
                            .and_then(|(user_timestamp, frame_id)| {
                                if user_timestamp == 0 && frame_id == 0 {
                                    None
                                } else {
                                    Some(crate::video_frame::FrameMetadata {
                                        user_timestamp: (user_timestamp != 0)
                                            .then_some(user_timestamp),
                                        frame_id: (frame_id != 0).then_some(frame_id),
                                    })
                                }
                            })
                    }
                };

                metadata
            });
        self.frame_queue.push(AudioFrame {
            data: data.to_owned().into(),
            sample_rate,
            num_channels: nb_channels as u32,
            samples_per_channel: nb_frames as u32,
            timestamp,
            frame_metadata,
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

    use super::{
        derive_user_timestamp_us, is_newer_or_same_rtp_timestamp, AudioFrameQueue,
        AudioTimestampAnchor,
    };
    use crate::{audio_frame::AudioFrame, prelude::FrameMetadata};

    fn test_frame(marker: i16) -> AudioFrame<'static> {
        AudioFrame {
            data: vec![marker].into(),
            sample_rate: 48_000,
            num_channels: 1,
            samples_per_channel: 1,
            timestamp: None,
            frame_metadata: None,
        }
    }

    fn pop_marker(queue: &AudioFrameQueue) -> Option<i16> {
        queue.try_pop().map(|frame| frame.data[0])
    }

    fn test_frame_with_metadata(
        marker: i16,
        user_timestamp: u64,
        frame_id: u32,
    ) -> AudioFrame<'static> {
        AudioFrame {
            data: vec![marker].into(),
            sample_rate: 48_000,
            num_channels: 1,
            samples_per_channel: 1,
            timestamp: Some(crate::audio_frame::AudioFrameTimestamp {
                rtp_timestamp: user_timestamp as u32,
            }),
            frame_metadata: Some(FrameMetadata {
                user_timestamp: Some(user_timestamp),
                frame_id: Some(frame_id),
            }),
        }
    }

    fn pop_metadata(queue: &AudioFrameQueue) -> Option<(u64, u32)> {
        queue.try_pop().and_then(|frame| {
            frame.frame_metadata.map(|metadata| {
                (
                    metadata.user_timestamp.expect("user timestamp"),
                    metadata.frame_id.expect("frame id"),
                )
            })
        })
    }

    fn pop_timestamp(queue: &AudioFrameQueue) -> Option<u32> {
        queue.try_pop().and_then(|frame| frame.timestamp.map(|timestamp| timestamp.rtp_timestamp))
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

    #[test]
    fn bounded_queue_preserves_metadata_for_kept_frames() {
        let queue = AudioFrameQueue::new(Some(2));

        queue.push(test_frame_with_metadata(1, 100, 10));
        queue.push(test_frame_with_metadata(2, 200, 20));

        assert_eq!(pop_metadata(&queue), Some((100, 10)));
        assert_eq!(pop_metadata(&queue), Some((200, 20)));
    }

    #[test]
    fn bounded_queue_drops_oldest_metadata_with_oldest_frame() {
        let queue = AudioFrameQueue::new(Some(2));

        queue.push(test_frame_with_metadata(1, 100, 10));
        queue.push(test_frame_with_metadata(2, 200, 20));
        queue.push(test_frame_with_metadata(3, 300, 30));

        assert_eq!(pop_metadata(&queue), Some((200, 20)));
        assert_eq!(pop_metadata(&queue), Some((300, 30)));
    }

    #[test]
    fn bounded_queue_preserves_timestamps_for_kept_frames() {
        let queue = AudioFrameQueue::new(Some(2));

        queue.push(test_frame_with_metadata(1, 100, 10));
        queue.push(test_frame_with_metadata(2, 200, 20));

        assert_eq!(pop_timestamp(&queue), Some(100));
        assert_eq!(pop_timestamp(&queue), Some(200));
    }

    #[test]
    fn first_anchor_is_used_as_interpolation_base() {
        let anchor = AudioTimestampAnchor {
            rtp_timestamp: 48_000,
            user_timestamp_us: 2_000_000,
            frame_id: Some(7),
            sample_rate: 48_000,
        };

        assert_eq!(derive_user_timestamp_us(anchor, 48_480), 2_010_000);
        assert_eq!(derive_user_timestamp_us(anchor, 48_960), 2_020_000);
    }

    #[test]
    fn interpolation_remains_monotonic_for_sequential_frames() {
        let anchor = AudioTimestampAnchor {
            rtp_timestamp: 1_000,
            user_timestamp_us: 5_000_000,
            frame_id: None,
            sample_rate: 48_000,
        };

        let first = derive_user_timestamp_us(anchor, 1_480);
        let second = derive_user_timestamp_us(anchor, 1_960);
        let third = derive_user_timestamp_us(anchor, 2_440);

        assert_eq!(first, 5_010_000);
        assert_eq!(second, 5_020_000);
        assert_eq!(third, 5_030_000);
        assert!(first < second && second < third);
    }

    #[test]
    fn newer_anchor_replaces_prior_anchor_in_rtp_sequence_space() {
        assert!(is_newer_or_same_rtp_timestamp(10_000, 10_480));
        assert!(is_newer_or_same_rtp_timestamp(10_000, 10_000));
        assert!(!is_newer_or_same_rtp_timestamp(10_480, 10_000));
    }

    #[test]
    fn wraparound_rtp_delta_interpolates_forward() {
        let anchor = AudioTimestampAnchor {
            rtp_timestamp: u32::MAX - 239,
            user_timestamp_us: 9_000_000,
            frame_id: None,
            sample_rate: 48_000,
        };

        assert_eq!(derive_user_timestamp_us(anchor, 240), 9_010_000);
        assert!(is_newer_or_same_rtp_timestamp(u32::MAX - 239, 240));
    }

    #[test]
    fn interpolation_requires_an_anchor() {
        let anchor: Option<AudioTimestampAnchor> = None;
        assert!(anchor.map(|anchor| derive_user_timestamp_us(anchor, 480)).is_none());
    }
}
