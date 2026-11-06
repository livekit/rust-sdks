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

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use futures_util::StreamExt;
use livekit::{
    prelude::Track,
    webrtc::{
        prelude::*,
        video_stream::native::{NativeVideoStream, NativeVideoStreamOptions},
    },
};
use tokio::sync::{broadcast, mpsc, oneshot};

use super::{colorcvt, room::FfiTrack, FfiHandle};
use crate::server::utils;
use crate::{proto, server, FfiError, FfiHandleId, FfiResult};
use livekit::webrtc::video_frame::FrameMetadata;

pub struct FfiVideoStream {
    pub handle_id: FfiHandleId,
    pub stream_type: proto::VideoStreamType,

    #[allow(dead_code)]
    self_dropped_tx: oneshot::Sender<()>, // Close the stream on drop
}

impl FfiHandle for FfiVideoStream {}

/// Frames an FFI video stream has lost, totalled across every track it
/// forwards.
///
/// A participant stream keeps one handle while the participant swaps tracks,
/// and each track gets a fresh `NativeVideoStream` whose own counter starts at
/// zero. Carrying finished tracks' losses here is what keeps the number the
/// application sees cumulative across a replacement.
#[derive(Clone, Default)]
struct DropTotal(Arc<AtomicU64>);

impl DropTotal {
    /// Losses of earlier tracks plus `current`, the live track's own count.
    fn with_current(&self, current: u64) -> u64 {
        self.0.load(Ordering::Relaxed) + current
    }

    /// Folds a finished track's losses in, so the next track counts on top.
    fn finish_track(&self, current: u64) {
        self.0.fetch_add(current, Ordering::Relaxed);
    }
}

/// Upper bound on the frame buffers one stream keeps alive.
///
/// Sized for how many frames a consumer holds at once in steady state: one for
/// a binding that copies and releases immediately, two for a renderer holding
/// the frame it is displaying while the next arrives, and one of slack for
/// jitter. Going beyond that means a sustained backlog, where the consumer has
/// already pinned the memory itself and the allocation is not what is hurting.
///
/// Kept small because the set never shrinks, so a burst would otherwise pin its
/// high-water mark for the life of the stream -- 12 MB per 4K frame. Past the
/// cap the stream allocates per frame, exactly as it did before.
const MAX_FRAME_BUFFERS: usize = 3;

/// The destination buffers one video stream converts into.
///
/// Converting a frame used to allocate a fresh buffer the size of the frame and
/// free it when the binding released the handle -- 4.5 MB per frame at
/// 1628x1920 I420, 134 MB/s at 30 fps. These are kept and handed out again
/// instead.
///
/// A buffer may only be written while nothing else holds it. `Arc` tracks
/// that: the FFI handle map holds a clone for as long as the binding has the
/// frame, so a strong count of 1 means the buffer came back and is safe to
/// reuse. Nothing relies on the binding releasing frames promptly -- if none
/// of the buffers is free, [`acquire`](Self::acquire) adds one.
#[derive(Default)]
struct FrameBuffers {
    buffers: Vec<Arc<Vec<u8>>>,
}

impl FrameBuffers {
    /// Borrows a buffer no one else holds, reusing one when possible.
    ///
    /// The slot always has a strong count of 1, so the caller can take
    /// `Arc::get_mut` on it. Returning the slot rather than a clone is what
    /// makes that true -- a clone would be the second owner itself.
    fn acquire(&mut self) -> &mut Arc<Vec<u8>> {
        if let Some(index) = self.buffers.iter().position(|buffer| Arc::strong_count(buffer) == 1) {
            return &mut self.buffers[index];
        }

        if self.buffers.len() < MAX_FRAME_BUFFERS {
            self.buffers.push(Arc::new(Vec::new()));
            return self.buffers.last_mut().unwrap();
        }

        // Every buffer is still out and we are at the cap. Give up this slot:
        // the binding owns that buffer now and frees it on release, and we
        // start a fresh one here rather than block or overwrite it.
        self.buffers[0] = Arc::new(Vec::new());
        &mut self.buffers[0]
    }
}

fn frame_metadata_to_proto(metadata: Option<FrameMetadata>) -> Option<proto::FrameMetadata> {
    metadata.map(|metadata| proto::FrameMetadata {
        user_timestamp: metadata.user_timestamp,
        frame_id: metadata.frame_id,
        user_data: metadata.user_data,
    })
}

impl FfiVideoStream {
    /// Setup a new VideoStream and forward the frame data to the client/the foreign
    /// language.
    ///
    /// When FFIVideoStream is dropped (When the corresponding handle_id is dropped), the task
    /// is being closed.
    ///
    /// It is possible that the client receives a VideoFrame after the task is closed. The client
    /// musts ignore it.
    pub fn from_track(
        server: &'static server::FfiServer,
        new_stream: proto::NewVideoStreamRequest,
    ) -> FfiResult<proto::OwnedVideoStream> {
        let ffi_track = server.retrieve_handle::<FfiTrack>(new_stream.track_handle)?.clone();
        let rtc_track = ffi_track.track.rtc_track();

        let MediaStreamTrack::Video(rtc_track) = rtc_track else {
            return Err(FfiError::InvalidRequest("not a video track".into()));
        };

        let (self_dropped_tx, self_dropped_rx) = oneshot::channel();
        let stream_type = new_stream.r#type();
        let handle_id = server.next_id();
        let stream = match stream_type {
            #[cfg(not(target_arch = "wasm32"))]
            proto::VideoStreamType::VideoStreamNative => {
                let video_stream = Self { handle_id, self_dropped_tx, stream_type };
                let options = NativeVideoStreamOptions {
                    queue_size_frames: new_stream
                        .queue_size_frames
                        .map(|capacity| capacity as usize),
                };
                let handle = server.async_runtime.spawn(Self::native_video_stream_task(
                    server,
                    handle_id,
                    new_stream.format.and_then(|_| Some(new_stream.format())),
                    new_stream.normalize_stride.unwrap_or(true),
                    NativeVideoStream::with_options(rtc_track, options),
                    self_dropped_rx,
                    server.watch_handle_dropped(new_stream.track_handle),
                    true,
                    DropTotal::default(),
                ));
                server.watch_panic(handle);
                Ok::<FfiVideoStream, FfiError>(video_stream)
            }
            _ => return Err(FfiError::InvalidRequest("unsupported video stream type".into())),
        }?;

        // Store the new video stream and return the info
        let info = proto::VideoStreamInfo::from(&stream);
        server.store_handle(stream.handle_id, stream);

        Ok(proto::OwnedVideoStream { handle: proto::FfiOwnedHandle { id: handle_id }, info })
    }

    pub fn from_participant(
        server: &'static server::FfiServer,
        request: proto::VideoStreamFromParticipantRequest,
    ) -> FfiResult<proto::OwnedVideoStream> {
        let (self_dropped_tx, self_dropped_rx) = oneshot::channel();
        let stream_type = request.r#type();
        let handle_id = server.next_id();
        let dst_type = request.format.and_then(|_| Some(request.format()));
        let stream = match stream_type {
            #[cfg(not(target_arch = "wasm32"))]
            proto::VideoStreamType::VideoStreamNative => {
                let video_stream = Self { handle_id, self_dropped_tx, stream_type };
                let handle = server.async_runtime.spawn(Self::participant_video_stream_task(
                    server,
                    request,
                    handle_id,
                    dst_type,
                    self_dropped_rx,
                ));
                server.watch_panic(handle);
                Ok::<FfiVideoStream, FfiError>(video_stream)
            }
            _ => return Err(FfiError::InvalidRequest("unsupported video stream type".into())),
        }?;
        let info = proto::VideoStreamInfo::from(&stream);
        server.store_handle(stream.handle_id, stream);

        Ok(proto::OwnedVideoStream { handle: proto::FfiOwnedHandle { id: handle_id }, info: info })
    }

    async fn native_video_stream_task(
        server: &'static server::FfiServer,
        stream_handle: FfiHandleId,
        dst_type: Option<proto::VideoBufferType>,
        normalize_stride: bool,
        mut native_stream: NativeVideoStream,
        mut self_dropped_rx: oneshot::Receiver<()>,
        mut handle_dropped_rx: oneshot::Receiver<()>,
        send_eos: bool,
        dropped_before: DropTotal,
    ) {
        let mut buffers = FrameBuffers::default();

        loop {
            tokio::select! {
                _ = &mut self_dropped_rx => {
                    break;
                }
                _ = &mut handle_dropped_rx => {
                    break;
                }
                frame = native_stream.next() => {
                    let Some(frame) = frame else {
                        break;
                    };

                    let metadata = frame_metadata_to_proto(frame.frame_metadata);
                    let timestamp_us = frame.timestamp_us;
                    let rotation = proto::VideoRotation::from(frame.rotation).into();
                    let slot = buffers.acquire();
                    let Ok(info) = colorcvt::to_video_buffer_info(
                        frame.buffer,
                        dst_type,
                        normalize_stride,
                        Arc::get_mut(slot).expect("acquire yields an unshared buffer"),
                    ) else {
                        log::error!("video stream failed to convert video frame to {:?}", dst_type);
                        continue;
                    };

                    let handle_id = server.next_id();
                    // The handle co-owns the buffer. That second reference is
                    // what stops the buffer being written again before the
                    // binding is done reading it.
                    server.store_handle(handle_id, slot.clone());

                    // Read after the frame is popped, so the count covers every
                    // drop that happened before this frame was delivered. The
                    // base carries the losses of tracks this FFI stream
                    // forwarded earlier, which keeps the total cumulative
                    // across a track replacement.
                    let dropped_frames = dropped_before.with_current(native_stream.dropped_frames());

                    if let Err(err) = server.send_event(
                        proto::VideoStreamEvent {
                            stream_handle,
                            message: Some(proto::video_stream_event::Message::FrameReceived(
                                proto::VideoFrameReceived {
                                    timestamp_us,
                                    rotation,
                                    buffer: proto::OwnedVideoBuffer {
                                        handle: proto::FfiOwnedHandle {
                                            id: handle_id,
                                        },
                                        info,
                                    },
                                    metadata,
                                    dropped_frames: Some(dropped_frames),
                                }
                            )),
                        }.into()
                    ) {
                        server.drop_handle(handle_id);
                        log::warn!("failed to send video frame: {}", err);
                    }
                }
            }
        }

        // This track is done; its losses belong to the stream's running total.
        dropped_before.finish_track(native_stream.dropped_frames());

        if send_eos {
            if let Err(err) = server.send_event(
                proto::VideoStreamEvent {
                    stream_handle,
                    message: Some(proto::video_stream_event::Message::Eos(
                        proto::VideoStreamEos {},
                    )),
                }
                .into(),
            ) {
                log::warn!("failed to send video EOS: {}", err);
            }
        }
    }

    async fn participant_video_stream_task(
        server: &'static server::FfiServer,
        request: proto::VideoStreamFromParticipantRequest,
        stream_handle: FfiHandleId,
        dst_type: Option<proto::VideoBufferType>,
        mut close_rx: oneshot::Receiver<()>,
    ) {
        let ffi_participant =
            utils::ffi_participant_from_handle(server, request.participant_handle);
        let ffi_participant = match ffi_participant {
            Ok(ffi_participant) => ffi_participant,
            Err(err) => {
                log::error!("failed to get participant: {}", err);
                return;
            }
        };

        let track_source = request.track_source();
        let normalize_stride = request.normalize_stride.unwrap_or(true);
        let queue_size_frames = request.queue_size_frames.map(|capacity| capacity as usize);
        // One FFI stream handle spans every track this participant publishes,
        // so the drop total has to survive a replacement.
        let dropped_before = DropTotal::default();
        let (track_tx, mut track_rx) = mpsc::channel::<Track>(1);
        let (track_finished_tx, track_finished_rx) = broadcast::channel::<Track>(1);
        server.async_runtime.spawn(utils::track_changed_trigger(
            ffi_participant,
            track_source.into(),
            track_tx,
            track_finished_tx.clone(),
        ));
        // track_tx is no longer held, so the track_rx will be closed when track_changed_trigger is done

        loop {
            let track = track_rx.recv().await;
            if let Some(track) = track {
                let rtc_track = track.rtc_track();
                let MediaStreamTrack::Video(rtc_track) = rtc_track else {
                    continue;
                };
                let (c_tx, c_rx) = oneshot::channel::<()>();
                let (handle_dropped_tx, handle_dropped_rx) = oneshot::channel::<()>();
                let (done_tx, mut done_rx) = oneshot::channel::<()>();
                let options = NativeVideoStreamOptions { queue_size_frames };

                let dropped_before = dropped_before.clone();
                let mut track_finished_rx = track_finished_tx.subscribe();
                server.async_runtime.spawn(async move {
                    tokio::select! {
                            t = track_finished_rx.recv() => {
                            let Ok(t) = t else {
                                return
                            };
                            if t.sid() == track.sid() {
                                handle_dropped_tx.send(()).ok();
                                return
                            }
                        }
                    }
                });

                server.async_runtime.spawn(async move {
                    Self::native_video_stream_task(
                        server,
                        stream_handle,
                        dst_type,
                        normalize_stride,
                        NativeVideoStream::with_options(rtc_track, options),
                        c_rx,
                        handle_dropped_rx,
                        false,
                        dropped_before,
                    )
                    .await;
                    let _ = done_tx.send(());
                });
                tokio::select! {
                    _ = &mut close_rx => {
                        let _ = c_tx.send(());
                        return
                    }
                    _ = &mut done_rx => {
                        continue
                    }
                }
            } else {
                // when tracks are done (i.e. the participant leaves the room), we are done
                break;
            }
        }
        if let Err(err) = server.send_event(
            proto::VideoStreamEvent {
                stream_handle,
                message: Some(proto::video_stream_event::Message::Eos(proto::VideoStreamEos {})),
            }
            .into(),
        ) {
            log::warn!("failed to send video EOS: {}", err);
        }
    }
}

#[cfg(test)]
mod tests {
    use livekit::webrtc::video_frame::FrameMetadata;

    use super::*;

    #[test]
    fn missing_frame_metadata_stays_missing() {
        assert!(frame_metadata_to_proto(None).is_none());
    }

    #[test]
    fn frame_metadata_optionality_is_preserved() {
        let metadata = frame_metadata_to_proto(Some(FrameMetadata {
            user_timestamp: Some(123),
            frame_id: None,
            user_data: Some(vec![1, 2, 3]),
        }))
        .unwrap();

        assert_eq!(metadata.user_timestamp, Some(123));
        assert_eq!(metadata.frame_id, None);
        assert_eq!(metadata.user_data, Some(vec![1, 2, 3]));
    }

    /// Stands in for the FFI handle map: holding this clone is what marks a
    /// buffer as still out with the binding.
    fn handed_to_binding(slot: &Arc<Vec<u8>>) -> Arc<Vec<u8>> {
        slot.clone()
    }

    #[test]
    fn a_released_buffer_is_handed_out_again() {
        let mut buffers = FrameBuffers::default();

        let first = buffers.acquire();
        Arc::get_mut(first).expect("unshared").extend_from_slice(&[1, 2, 3]);
        let allocation = Arc::as_ptr(first);
        let held = handed_to_binding(first);
        drop(held); // the binding released the frame

        let second = buffers.acquire();

        assert_eq!(Arc::as_ptr(second), allocation, "the same allocation came back");
        assert_eq!(second.as_slice(), &[1, 2, 3], "and it is the same storage");
        assert_eq!(buffers.buffers.len(), 1, "no second buffer was needed");
    }

    #[test]
    fn a_buffer_still_held_by_the_binding_is_never_handed_out() {
        let mut buffers = FrameBuffers::default();

        let first = buffers.acquire();
        // Identity by allocation, not by data pointer: an empty Vec's data
        // pointer is a shared dangling sentinel and compares equal.
        let first_allocation = Arc::as_ptr(first);
        let _held = handed_to_binding(first); // binding still has frame 1

        let second = buffers.acquire();

        assert_ne!(Arc::as_ptr(second), first_allocation, "writing here would corrupt frame 1");
        assert!(Arc::get_mut(second).is_some(), "the caller can write to it");
        assert_eq!(buffers.buffers.len(), 2);
    }

    #[test]
    fn two_buffers_suffice_when_one_frame_is_held_at_a_time() {
        // What a zero-copy binding does: hold the current frame until the next
        // one arrives, then release it.
        let mut buffers = FrameBuffers::default();
        let mut previous = None;

        for _ in 0..16 {
            let slot = buffers.acquire();
            assert!(Arc::get_mut(slot).is_some());
            let held = handed_to_binding(slot);
            previous = Some(held); // releases the frame before it, as Unity does
        }
        drop(previous);

        assert_eq!(buffers.buffers.len(), 2, "alternating between two is enough");
    }

    #[test]
    fn growth_stops_at_the_cap() {
        let mut buffers = FrameBuffers::default();

        // Never release, so every buffer stays out.
        let held: Vec<_> =
            (0..MAX_FRAME_BUFFERS + 4).map(|_| handed_to_binding(buffers.acquire())).collect();

        assert_eq!(buffers.buffers.len(), MAX_FRAME_BUFFERS);
        assert_eq!(held.len(), MAX_FRAME_BUFFERS + 4, "every frame still got a buffer");
        // Past the cap each frame gets its own, so no two frames share memory.
        let mut allocations: Vec<_> = held.iter().map(Arc::as_ptr).collect();
        allocations.sort_unstable();
        let total = allocations.len();
        allocations.dedup();
        assert_eq!(allocations.len(), total, "buffers handed out at once must be distinct");
    }

    #[test]
    fn the_drop_total_carries_across_a_track_replacement() {
        let total = DropTotal::default();

        assert_eq!(total.with_current(12), 12, "first track, 12 of its own");
        total.finish_track(12); // the participant swaps tracks here

        // The next track's NativeVideoStream starts its own counter at zero.
        assert_eq!(total.with_current(0), 12, "the swap must not reset the total");
        assert_eq!(total.with_current(3), 15, "and the new track counts on top");
    }

    #[test]
    fn a_resized_frame_reuses_the_same_buffer() {
        // A simulcast layer switch changes the frame size; the allocation
        // should still be reused rather than replaced.
        let mut buffers = FrameBuffers::default();

        let first = buffers.acquire();
        Arc::get_mut(first).expect("unshared").resize(1024, 0);
        drop(handed_to_binding(first));

        let second = buffers.acquire();
        Arc::get_mut(second).expect("unshared").resize(2048, 0);

        assert_eq!(buffers.buffers.len(), 1);
        assert_eq!(buffers.buffers[0].len(), 2048);
    }
}
