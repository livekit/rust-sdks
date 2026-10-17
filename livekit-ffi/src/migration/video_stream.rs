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

//! Everything holding [`crate::server::video_stream::VideoStream`] onto the protobuf path:
//! the handle bridge, the request handlers, and the event senders that forward what
//! `next()` yields.
//!
//! This whole file goes when the protobuf path does. What it leaves behind over there is
//! the `handle_id` field on the struct, and the `pub(crate)` these reach through.

use livekit::{
    prelude::Track,
    webrtc::{
        prelude::*,
        video_stream::native::{NativeVideoStream, NativeVideoStreamOptions},
    },
};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::server::utils;
use crate::server::video_stream::{
    VideoBuffer, VideoBufferType, VideoFrame, VideoStream, VideoStreamError,
};
use crate::{proto, server, FfiError, FfiHandleId, FfiResult};

crate::migrate_from_ffi!(VideoStream);

impl VideoBuffer {
    /// Publishes the pixels to the FFI handle map, and describes them the way the
    /// protobuf path expects: a pointer per plane, into bytes the returned handle keeps
    /// alive. They are boxed first, so the pointers are only as stable as that handle.
    fn into_ffi(
        mut self,
        server: &'static server::FfiServer,
    ) -> (FfiHandleId, proto::VideoBufferInfo) {
        let data = std::mem::take(&mut self.data).into_boxed_slice();
        let base = data.as_ptr() as u64;
        let info = proto::VideoBufferInfo {
            r#type: proto::VideoBufferType::from(self.r#type) as i32,
            width: self.width,
            height: self.height,
            data_ptr: base,
            stride: self.stride,
            components: self
                .components
                .iter()
                .map(|component| proto::video_buffer_info::ComponentInfo {
                    data_ptr: base + component.offset,
                    stride: component.stride,
                    size: component.size,
                })
                .collect(),
        };
        let handle_id = server.next_id();
        server.store_handle(handle_id, data);
        (handle_id, info)
    }
}

impl From<VideoStreamError> for FfiError {
    fn from(err: VideoStreamError) -> Self {
        FfiError::InvalidRequest(err.to_string().into())
    }
}

impl VideoStream {
    /// Forwards frames to the FFI event bus until either handle is dropped or the track ends.
    ///
    /// This is the protobuf path as an adapter over [`VideoStream::next`]: one puller, one
    /// conversion, and the pixels reach the handle map only on their way out.
    async fn pump(
        server: &'static server::FfiServer,
        stream: Arc<Self>,
        stream_handle: FfiHandleId,
        mut stream_dropped_rx: oneshot::Receiver<()>,
        mut track_dropped_rx: oneshot::Receiver<()>,
        send_eos: bool,
    ) {
        loop {
            tokio::select! {
                _ = &mut stream_dropped_rx => break,
                _ = &mut track_dropped_rx => break,
                frame = stream.next() => {
                    let Some(frame) = frame else {
                        break;
                    };
                    send_frame(server, stream_handle, frame);
                }
            }
        }

        if send_eos {
            send_eos_event(server, stream_handle);
        }
    }

    /// Setup a new VideoStream and forward the frame data to the client/the foreign
    /// language.
    ///
    /// The forwarding task stops when either the stream handle or the track handle is
    /// dropped. It holds an `Arc` of the stream, so the handle going away is what ends
    /// it — the stream itself only drops once the task has let go.
    ///
    /// It is possible that the client receives a VideoFrame after the task is closed. The
    /// client musts ignore it.
    pub fn from_track(
        server: &'static server::FfiServer,
        new_stream: proto::NewVideoStreamRequest,
    ) -> FfiResult<proto::OwnedVideoStream> {
        if new_stream.r#type() != proto::VideoStreamType::VideoStreamNative {
            return Err(FfiError::InvalidRequest("unsupported video stream type".into()));
        }

        let track_handle = new_stream.track_handle;
        let stream = Self::from_track_handle(
            track_handle,
            new_stream.format.map(|_| new_stream.format().into()),
            new_stream.normalize_stride.unwrap_or(true),
            new_stream.queue_size_frames,
        )?;

        let info = video_stream_info();
        let handle_id = stream.clone().ffi_handle_id();
        let task = server.async_runtime.spawn(Self::pump(
            server,
            stream,
            handle_id,
            server.watch_handle_dropped(handle_id),
            server.watch_handle_dropped(track_handle),
            true,
        ));
        server.watch_panic(task);

        Ok(proto::OwnedVideoStream { handle: proto::FfiOwnedHandle { id: handle_id }, info })
    }

    /// Follows a participant's track source, restarting the stream each time the track is
    /// republished.
    ///
    /// Still protobuf-only: the object holds no stream of its own, so [`VideoStream::next`]
    /// yields nothing on it. Migrating it means teaching `next` to wait out a track change
    /// without reporting end-of-stream, which is its own layer.
    pub fn from_participant(
        server: &'static server::FfiServer,
        request: proto::VideoStreamFromParticipantRequest,
    ) -> FfiResult<proto::OwnedVideoStream> {
        if request.r#type() != proto::VideoStreamType::VideoStreamNative {
            return Err(FfiError::InvalidRequest("unsupported video stream type".into()));
        }

        let dst_type = request.format.map(|_| request.format().into());
        let normalize_stride = request.normalize_stride.unwrap_or(true);
        let stream = Arc::new(Self::over(None, dst_type, normalize_stride));

        let info = video_stream_info();
        let handle_id = stream.ffi_handle_id();
        let task = server.async_runtime.spawn(Self::participant_video_stream_task(
            server,
            request,
            handle_id,
            dst_type,
            normalize_stride,
            server.watch_handle_dropped(handle_id),
        ));
        server.watch_panic(task);

        Ok(proto::OwnedVideoStream { handle: proto::FfiOwnedHandle { id: handle_id }, info })
    }

    async fn participant_video_stream_task(
        server: &'static server::FfiServer,
        request: proto::VideoStreamFromParticipantRequest,
        stream_handle: FfiHandleId,
        dst_type: Option<VideoBufferType>,
        normalize_stride: bool,
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
        let queue_size_frames = request.queue_size_frames.map(|capacity| capacity as usize);
        let (track_tx, mut track_rx) = mpsc::channel::<Track>(1);
        let (track_finished_tx, _track_finished_rx) = broadcast::channel::<Track>(1);
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

                // One stream per track, pumped by the same loop as a track-sourced stream.
                // It is never published as a handle: the outer stream owns the id.
                let per_track = Arc::new(Self::over(
                    Some(NativeVideoStream::with_options(rtc_track, options)),
                    dst_type,
                    normalize_stride,
                ));
                server.async_runtime.spawn(async move {
                    Self::pump(server, per_track, stream_handle, c_rx, handle_dropped_rx, false)
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

        send_eos_event(server, stream_handle);
    }
}

/// Every stream this crate builds is native: `from_track` and `from_participant` reject
/// the browser-side types.
fn video_stream_info() -> proto::VideoStreamInfo {
    proto::VideoStreamInfo { r#type: proto::VideoStreamType::VideoStreamNative as i32 }
}

fn send_frame(server: &'static server::FfiServer, stream_handle: FfiHandleId, frame: VideoFrame) {
    let VideoFrame { timestamp_us, rotation, metadata, buffer } = frame;
    let (handle_id, info) = buffer.into_ffi(server);

    if let Err(err) = server.send_event(
        proto::VideoStreamEvent {
            stream_handle,
            message: Some(proto::video_stream_event::Message::FrameReceived(
                proto::VideoFrameReceived {
                    timestamp_us,
                    rotation: proto::VideoRotation::from(rotation).into(),
                    buffer: proto::OwnedVideoBuffer {
                        handle: proto::FfiOwnedHandle { id: handle_id },
                        info,
                    },
                    metadata: metadata.map(Into::into),
                },
            )),
        }
        .into(),
    ) {
        server.drop_handle(handle_id);
        log::warn!("failed to send video frame: {}", err);
    }
}

fn send_eos_event(server: &'static server::FfiServer, stream_handle: FfiHandleId) {
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

/// Direct-call tests for [`VideoStream`] crossing the FFI seam, paired with the
/// protobuf-driven tests in `crate::migration_tests`.
///
/// The probe for "one object, two surfaces" is the stream's own queue: a frame pulled
/// through one `Arc` is gone from the other, the way the resampler's filter state is the
/// probe over there. It needs real frames, so these drive a local video track end to end.
#[cfg(test)]
mod migration_tests {
    use super::*;
    use crate::proto;
    use crate::server::video_stream::VideoBufferComponent;
    use crate::{server::room::FfiTrack, FFI_SERVER};
    use livekit::prelude::LocalVideoTrack;
    use livekit::webrtc::{
        video_frame::{self as rtc, I420Buffer},
        video_source::{native::NativeVideoSource, RtcVideoSource, VideoResolution},
    };
    use std::time::Duration;

    const WIDTH: u32 = 64;
    const HEIGHT: u32 = 32;

    /// A track fed by a source the test drives directly, published as an FFI handle the
    /// way `create_video_track` publishes one.
    fn video_track() -> (NativeVideoSource, FfiHandleId) {
        let source =
            NativeVideoSource::new(VideoResolution { width: WIDTH, height: HEIGHT }, false);
        let track =
            LocalVideoTrack::create_video_track("probe", RtcVideoSource::Native(source.clone()));
        let handle = FFI_SERVER.next_id();
        FFI_SERVER.store_handle(
            handle,
            FfiTrack { handle, track: Track::LocalVideo(track), room_handle: None },
        );
        (source, handle)
    }

    /// A frame of flat luma, tagged so it can be told apart from its neighbours — and from
    /// the black keepalive frames `NativeVideoSource` injects until the first real capture.
    fn capture(source: &NativeVideoSource, timestamp_us: i64, luma: u8) {
        assert_ne!(luma, 0, "a black frame is indistinguishable from a keepalive");
        let mut buffer = I420Buffer::new(WIDTH, HEIGHT);
        let (y, u, v) = buffer.data_mut();
        y.fill(luma);
        u.fill(128);
        v.fill(128);
        let frame = rtc::VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us,
            frame_metadata: None,
            buffer,
        };
        assert!(source.capture_frame(&frame), "the adapter dropped the frame");
    }

    /// The next tagged frame, skipping any keepalive still in the queue.
    async fn next_tagged(stream: &VideoStream) -> VideoFrame {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let frame = stream.next().await.expect("the stream ended");
                if frame.buffer.data[0] != 0 {
                    return frame;
                }
            }
        })
        .await
        .expect("timed out waiting for a tagged frame")
    }

    /// A stream created over uniffi publishes a handle, the handle resolves back to the
    /// very same object, and the two `Arc`s share one queue: pulling through either
    /// advances it for both. `take_ffi_handle_id` hands sole ownership back, and
    /// publishing again restores the same id with the stream still running.
    #[serial_test::parallel]
    #[test]
    fn a_video_stream_hands_back_and_forth_across_the_seam() {
        FFI_SERVER.async_runtime.block_on(async {
            let (source, track_handle) = video_track();
            let stream = VideoStream::from_track_handle(
                track_handle,
                Some(VideoBufferType::I420),
                true,
                None,
            )
            .expect("the track handle resolves to a video track");
            assert_eq!(Arc::strong_count(&stream), 1, "an unpublished stream has no other owner");

            let handle = stream.clone().ffi_handle_id();
            let same = VideoStream::from_ffi_handle_id(handle).expect("the published id resolves");
            assert!(Arc::ptr_eq(&same, &stream), "the id resolves to the object that published it");

            // A second stream over the same track, to show the check below discriminates.
            let rival = VideoStream::from_track_handle(
                track_handle,
                Some(VideoBufferType::I420),
                true,
                None,
            )
            .expect("a second stream over the same track");

            // One frame, and the two Arcs go looking for it. Being one object they share the
            // one NativeVideoStream, so the pull through `stream` takes the frame out from
            // under `same`.
            capture(&source, 0, 10);
            assert_eq!(next_tagged(&stream).await.buffer.data[0], 10);
            assert!(
                tokio::time::timeout(Duration::from_millis(500), next_tagged(&same)).await.is_err(),
                "the frame `stream` consumed is gone from `same`"
            );
            // `rival` is what the assertion above would look like if they were two objects:
            // a separate stream is its own sink, and still holds its own copy of that frame.
            assert_eq!(next_tagged(&rival).await.buffer.data[0], 10);

            // The FFI side lets go, and the uniffi side carries on alone.
            stream.take_ffi_handle_id().expect("the FFI side co-owned the stream");
            assert!(
                VideoStream::from_ffi_handle_id(handle).is_err(),
                "a released handle is absent from the map until it is published again"
            );
            capture(&source, 200_000, 30);
            assert_eq!(next_tagged(&stream).await.buffer.data[0], 30);

            // Publishing again restores the handle, id and all, over the same live stream.
            assert_eq!(stream.clone().ffi_handle_id(), handle, "republishing keeps the id");
            let republished =
                VideoStream::from_ffi_handle_id(handle).expect("the republished handle resolves");
            assert!(Arc::ptr_eq(&republished, &stream));
            capture(&source, 300_000, 40);
            assert_eq!(next_tagged(&republished).await.buffer.data[0], 40);

            FFI_SERVER.drop_handle(handle);
            FFI_SERVER.drop_handle(track_handle);
        });
    }

    /// A [`VideoBuffer`] has no handle of its own — it is a record, so the pixels are what
    /// crosses rather than a reference to an object both sides share. What the FFI side
    /// gets is a handle owning those very bytes, with the info addressing them in place.
    #[serial_test::parallel]
    #[test]
    fn a_buffer_publishes_the_very_bytes_it_carried() {
        let buffer = VideoBuffer {
            r#type: VideoBufferType::I420,
            width: 8,
            height: 4,
            stride: None,
            components: vec![VideoBufferComponent { offset: 32, stride: 4, size: 8 }],
            data: (0u8..48).collect(),
        };
        let carried = buffer.data.clone();

        let (handle, info) = buffer.into_ffi(&FFI_SERVER);

        let owned =
            FFI_SERVER.retrieve_handle::<Box<[u8]>>(handle).expect("the bytes are published");
        assert_eq!(&**owned, &carried[..], "the handle owns the bytes the record carried");
        assert_eq!(info.data_ptr, owned.as_ptr() as u64, "the info addresses them in place");

        // ...and the foreign side reads a plane back through the pointer it was handed.
        let plane =
            unsafe { std::slice::from_raw_parts(info.components[0].data_ptr as *const u8, 8) };
        assert_eq!(plane, &carried[32..40]);
        drop(owned);

        FFI_SERVER.drop_handle(handle);
    }

    /// An 8x4 I420 buffer: 32 bytes of luma, then 4x2 U and V planes of 8 bytes each.
    #[test]
    fn buffer_planes_survive_the_pointer_offset_round_trip() {
        let data: Box<[u8]> = (0u8..48).collect::<Vec<_>>().into_boxed_slice();
        let base = data.as_ptr() as u64;
        let plane = |offset: u64, stride: u32, size: u32| proto::video_buffer_info::ComponentInfo {
            data_ptr: base + offset,
            stride,
            size,
        };
        let info = proto::VideoBufferInfo {
            r#type: proto::VideoBufferType::I420 as i32,
            width: 8,
            height: 4,
            data_ptr: base,
            stride: None,
            components: vec![plane(0, 8, 32), plane(32, 4, 8), plane(40, 4, 8)],
        };

        let buffer = VideoBuffer::from_ffi(data, info);

        let offsets: Vec<u64> = buffer.components.iter().map(|c| c.offset).collect();
        assert_eq!(offsets, vec![0, 32, 40]);
        assert_eq!(&buffer.data[40..48], &[40, 41, 42, 43, 44, 45, 46, 47]);
    }
}
