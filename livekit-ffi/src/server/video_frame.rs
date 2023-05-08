use crate::{proto, server, FfiError, FfiHandleId, FfiResult};
use futures_util::StreamExt;
use livekit::prelude::*;
use livekit::webrtc::media_stream::MediaStreamTrack;
use livekit::webrtc::video_frame::{BoxVideoFrameBuffer, VideoFrame};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use log::warn;
use server::utils;
use tokio::sync::oneshot;

// ===== FFIVideoStream =====

pub struct FfiVideoStream {
    handle_id: FfiHandleId,
    stream_type: proto::VideoStreamType,
    track_sid: TrackSid,

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
        let track_sid: TrackSid = new_stream.track_sid.into();

        let room_handle = new_stream
            .room_handle
            .ok_or(FfiError::InvalidRequest("room_handle is empty"))?
            .id as FfiHandleId;

        let track = utils::find_remote_track(
            server,
            &track_sid,
            &new_stream.participant_sid.into(),
            room_handle,
        )?
        .rtc_track();

        let MediaStreamTrack::Video(track) = track else {
            return Err(FfiError::InvalidRequest("not a video track"));
        };

        let stream = match stream_type {
            proto::VideoStreamType::VideoStreamNative => {
                let video_stream = Self {
                    handle_id: server.next_id(),
                    close_tx,
                    stream_type,
                    track_sid,
                };
                tokio::spawn(Self::native_video_stream_task(
                    server,
                    video_stream.handle_id,
                    NativeVideoStream::new(track),
                    close_rx,
                ));
                Ok::<FfiVideoStream, FfiError>(video_stream)
            }
            // TODO(theomonnom): Support other stream types
            _ => return Err(FfiError::InvalidRequest("unsupported video stream type")),
        }?;

        // Store the new video stream and return the info
        let info = proto::VideoStreamInfo::from(&stream);
        server
            .ffi_handles()
            .insert(stream.handle_id, Box::new(stream));

        Ok(info)
    }

    pub fn handle_id(&self) -> FfiHandleId {
        self.handle_id
    }

    pub fn stream_type(&self) -> proto::VideoStreamType {
        self.stream_type
    }

    pub fn track_sid(&self) -> &TrackSid {
        &self.track_sid
    }

    async fn native_video_stream_task(
        server: &'static server::FfiServer,
        stream_handle_id: FfiHandleId,
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
                        .ffi_handles()
                        .insert(handle_id, Box::new(frame.buffer));

                    if let Err(err) = server.send_event(proto::ffi_event::Message::VideoStreamEvent(
                        proto::VideoStreamEvent {
                            handle: Some(stream_handle_id.into()),
                            message: Some(proto::video_stream_event::Message::FrameReceived(
                                proto::VideoFrameReceived {
                                    frame: Some(frame_info),
                                    buffer: Some(buffer_info),
                                }
                            )),
                        }
                    )) {
                        warn!("failed to send video frame: {}", err);
                    }
                }
            }
        }
    }
}

// ===== FFIVideoSource =====

pub struct FfiVideoSource {
    handle_id: FfiHandleId,
    source_type: proto::VideoSourceType,
    source: VideoSource,
}

#[derive(Clone)]
pub enum VideoSource {
    Native(NativeVideoSource),
}

impl FfiVideoSource {
    pub fn setup(
        server: &'static server::FfiServer,
        new_source: proto::NewVideoSourceRequest,
    ) -> FfiResult<proto::VideoSourceInfo> {
        let source_type = proto::VideoSourceType::from_i32(new_source.r#type).unwrap();
        let source_inner = match source_type {
            proto::VideoSourceType::VideoSourceNative => {
                let video_source = NativeVideoSource::default();
                Ok(VideoSource::Native(video_source))
            }
            _ => Err(FfiError::InvalidRequest("unsupported video source type")),
        }?;

        let video_source = Self {
            handle_id: server.next_id(),
            source_type,
            source: source_inner,
        };
        let source_info = proto::VideoSourceInfo::from(&video_source);

        server
            .ffi_handles()
            .insert(video_source.handle_id, Box::new(video_source));

        Ok(source_info)
    }

    pub fn capture_frame(
        &self,
        server: &'static server::FfiServer,
        capture: proto::CaptureVideoFrameRequest,
    ) -> FfiResult<()> {
        match self.source {
            VideoSource::Native(ref source) => {
                let frame_info = capture
                    .frame
                    .ok_or(FfiError::InvalidRequest("frame is empty"))?;

                let buffer_info = capture
                    .buffer
                    .ok_or(FfiError::InvalidRequest("buffer is none"))?;

                let handle_id = buffer_info
                    .handle
                    .ok_or(FfiError::InvalidRequest("handle is empty"))?
                    .id as FfiHandleId;

                let buffer = server
                    .ffi_handles()
                    .get(&handle_id)
                    .ok_or(FfiError::InvalidRequest("handle not found"))?;

                let buffer = buffer
                    .downcast_ref::<BoxVideoFrameBuffer>()
                    .ok_or(FfiError::InvalidRequest("handle is not video frame"))?;

                let rotation = proto::VideoRotation::from_i32(frame_info.rotation).unwrap();
                let frame = VideoFrame {
                    rotation: rotation.into(),
                    timestamp: frame_info.timestamp,
                    buffer,
                };

                source.capture_frame(&frame);
            }
        }
        Ok(())
    }

    pub fn handle_id(&self) -> FfiHandleId {
        self.handle_id
    }

    pub fn source_type(&self) -> proto::VideoSourceType {
        self.source_type
    }

    pub fn inner_source(&self) -> &VideoSource {
        &self.source
    }
}
