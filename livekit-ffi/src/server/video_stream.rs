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

use super::room::FfiTrack;
use super::FfiHandle;
use crate::{proto, server, FfiError, FfiHandleId, FfiResult};
use futures_util::StreamExt;
use livekit::webrtc::prelude::*;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use log::warn;
use tokio::sync::oneshot;

pub struct FfiVideoStream {
    handle_id: FfiHandleId,
    stream_type: proto::VideoStreamType,

    #[allow(dead_code)]
    close_tx: oneshot::Sender<()>, // Close the stream on drop
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
    pub fn setup(
        server: &'static server::FfiServer,
        new_stream: proto::NewVideoStreamRequest,
    ) -> FfiResult<proto::VideoStreamInfo> {
        let ffi_track = server.retrieve_handle::<FfiTrack>(new_stream.track_handle)?;
        let rtc_track = ffi_track.track.rtc_track();

        let MediaStreamTrack::Video(rtc_track) = rtc_track else {
            return Err(FfiError::InvalidRequest("not a video track"));
        };

        let (close_tx, close_rx) = oneshot::channel();
        let stream_type = proto::VideoStreamType::from_i32(new_stream.r#type).unwrap();
        let stream = match stream_type {
            #[cfg(not(target_arch = "wasm32"))]
            proto::VideoStreamType::VideoStreamNative => {
                let video_stream = Self {
                    handle_id: server.next_id(),
                    close_tx,
                    stream_type,
                };
                server.async_runtime.spawn(Self::native_video_stream_task(
                    server,
                    video_stream.handle_id,
                    NativeVideoStream::new(rtc_track),
                    close_rx,
                ));
                Ok::<FfiVideoStream, FfiError>(video_stream)
            }
            _ => return Err(FfiError::InvalidRequest("unsupported video stream type")),
        }?;

        // Store the new video stream and return the info
        let info = proto::VideoStreamInfo::from(
            proto::FfiOwnedHandle {
                id: stream.handle_id,
            },
            &stream,
        );
        server.store_handle(stream.handle_id, stream);
        Ok(info)
    }

    pub fn handle_id(&self) -> FfiHandleId {
        self.handle_id
    }

    pub fn stream_type(&self) -> proto::VideoStreamType {
        self.stream_type
    }

    async fn native_video_stream_task(
        server: &'static server::FfiServer,
        stream_handle: FfiHandleId,
        mut native_stream: NativeVideoStream,
        mut close_rx: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                _ = &mut close_rx => {
                    break;
                }
                frame = native_stream.next() => {
                    let Some(frame) = frame else {
                        break;
                    };

                    let handle_id = server.next_id();
                    let frame_info = proto::VideoFrameInfo::from(&frame);
                    let buffer_info = proto::VideoFrameBufferInfo::from(proto::FfiOwnedHandle { id: handle_id }, &frame.buffer);
                    server.store_handle(handle_id, frame.buffer);

                    if let Err(err) = server.send_event(proto::ffi_event::Message::VideoStreamEvent(
                        proto::VideoStreamEvent {
                            handle: stream_handle,
                            message: Some(proto::video_stream_event::Message::FrameReceived(
                                proto::VideoFrameReceived {
                                    frame: Some(frame_info),
                                    buffer: Some(buffer_info),
                                }
                            )),
                        }
                    )).await{
                        warn!("failed to send video frame: {}", err);
                    }
                }
            }
        }
    }
}
