use crate::{proto, server, FfiError, FfiHandleId, FfiResult};
use futures_util::StreamExt;
use livekit::prelude::*;
use livekit::webrtc::prelude::*;
use livekit::webrtc::video_frame::{BoxVideoFrameBuffer, VideoFrame};
use livekit::webrtc::video_stream::native::NativeVideoStream;
use log::warn;
use tokio::sync::oneshot;

pub struct FfiVideoStream {
    handle_id: FfiHandleId,
    stream_type: proto::VideoStreamType,

    #[allow(dead_code)]
    close_tx: oneshot::Sender<()>, // Close the stream on drop
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
    pub fn setup(
        server: &'static server::FfiServer,
        new_stream: proto::NewVideoStreamRequest,
    ) -> FfiResult<proto::VideoStreamInfo> {
        let (close_tx, close_rx) = oneshot::channel();
        let stream_type = proto::VideoStreamType::from_i32(new_stream.r#type).unwrap();

        let track = server
            .ffi_handles
            .get(&new_stream.track_handle)
            .ok_or(FfiError::InvalidRequest("track not found"))?;

        let track = track
            .downcast_ref::<Track>()
            .ok_or(FfiError::InvalidRequest("handle is not a Track"))?;

        let rtc_track = track.rtc_track();

        let MediaStreamTrack::Video(rtc_track) = rtc_track else {
            return Err(FfiError::InvalidRequest("not a video track"));
        };

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
        let info = proto::VideoStreamInfo::from(&stream);
        server
            .ffi_handles
            .insert(stream.handle_id, Box::new(stream));

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
                    let buffer_info = proto::VideoFrameBufferInfo::from(handle_id, &frame.buffer);

                    server
                        .ffi_handles
                        .insert(handle_id, Box::new(frame.buffer));

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

// ===== FFIVideoSource =====
