// Copyright 2023 LiveKit, Inc.
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

use futures_util::StreamExt;
use livekit::{
    prelude::Track,
    webrtc::{prelude::*, video_stream::native::NativeVideoStream},
};
use tokio::sync::{broadcast, mpsc, oneshot};

use super::{colorcvt, room::FfiTrack, FfiHandle};
use crate::server::utils;
use crate::{proto, server, FfiError, FfiHandleId, FfiResult};

pub struct FfiVideoStream {
    pub handle_id: FfiHandleId,
    pub stream_type: proto::VideoStreamType,

    #[allow(dead_code)]
    self_dropped_tx: oneshot::Sender<()>, // Close the stream on drop
}

impl FfiHandle for FfiVideoStream {}

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
                let handle = server.async_runtime.spawn(Self::native_video_stream_task(
                    server,
                    handle_id,
                    new_stream.format.and_then(|_| Some(new_stream.format())),
                    new_stream.normalize_stride.unwrap_or(true),
                    NativeVideoStream::new(rtc_track),
                    self_dropped_rx,
                    server.watch_handle_dropped(new_stream.track_handle),
                    true,
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
    ) {
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

                    let Ok((buffer, info)) = colorcvt::to_video_buffer_info(frame.buffer, dst_type, normalize_stride) else {
                        log::error!("video stream failed to convert video frame to {:?}", dst_type);
                        continue;
                    };

                    let handle_id = server.next_id();
                    server.store_handle(handle_id, buffer);


                    if let Err(err) = server.send_event(
                        proto::VideoStreamEvent {
                            stream_handle,
                            message: Some(proto::video_stream_event::Message::FrameReceived(
                                proto::VideoFrameReceived {
                                    timestamp_us: frame.timestamp_us,
                                    rotation: proto::VideoRotation::from(frame.rotation).into(),
                                    buffer: proto::OwnedVideoBuffer {
                                        handle: proto::FfiOwnedHandle {
                                            id: handle_id,
                                        },
                                        info,
                                    },
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
                        request.normalize_stride.unwrap_or(true),
                        NativeVideoStream::new(rtc_track),
                        c_rx,
                        handle_dropped_rx,
                        false,
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
