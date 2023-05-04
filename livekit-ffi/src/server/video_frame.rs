use crate::{proto, server, FFIError, FFIHandleId, FFIResult};
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

pub struct FFIVideoStream {
    handle_id: FFIHandleId,
    stream_type: proto::VideoStreamType,
    track_sid: TrackSid,

    #[allow(dead_code)]
    close_tx: oneshot::Sender<()>, // Close the stream on drop
}

impl FFIVideoStream {
    /// Setup a new VideoStream and forward the frame data to the client/the foreign
    /// language.
    ///
    /// When FFIVideoStream is dropped (When the corresponding handle_id is dropped), the task
    /// is being closed.
    ///
    /// It is possible that the client receives a VideoFrame after the task is closed. The client
    /// musts ignore it.
    pub fn setup(
        server: &'static server::FFIServer,
        new_stream: proto::NewVideoStreamRequest,
    ) -> FFIResult<proto::VideoStreamInfo> {
        let (close_tx, close_rx) = oneshot::channel();
        let stream_type = proto::VideoStreamType::from_i32(new_stream.r#type).unwrap();
        let track_sid: TrackSid = new_stream.track_sid.into();

        let track = utils::find_remote_track(
            server,
            &track_sid,
            &new_stream.participant_sid.into(),
            &new_stream.room_sid.into(),
        )?
        .rtc_track();

        let MediaStreamTrack::Video(track) = track else {
            return Err(FFIError::InvalidRequest("not a video track"));
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
                Ok(video_stream)
            }
            // TODO(theomonnom): Support other stream types
            _ => return Err(FFIError::InvalidRequest("unsupported video stream type")),
        }?;

        // Store the new video stream and return the info
        let info = proto::VideoStreamInfo::from(&stream);
        server
            .ffi_handles()
            .write()
            .insert(stream.handle_id, Box::new(stream));

        Ok(info)
    }

    pub fn handle_id(&self) -> FFIHandleId {
        self.handle_id
    }

    pub fn stream_type(&self) -> proto::VideoStreamType {
        self.stream_type
    }

    pub fn track_sid(&self) -> &TrackSid {
        &self.track_sid
    }

    async fn native_video_stream_task(
        server: &'static server::FFIServer,
        stream_handle_id: FFIHandleId,
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
                        .write()
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

pub struct FFIVideoSource {
    handle_id: FFIHandleId,
    source_type: proto::VideoSourceType,
    source: VideoSourceInner,
}

enum VideoSourceInner {
    Native(NativeVideoSource),
}

impl FFIVideoSource {
    pub fn setup(
        server: &'static server::FFIServer,
        new_source: proto::NewVideoSourceRequest,
    ) -> FFIResult<proto::VideoSourceInfo> {
        let source_type = proto::VideoSourceType::from_i32(new_source.r#type).unwrap();
        let source_inner = match source_type {
            proto::VideoSourceType::VideoSourceNative => {
                let video_source = NativeVideoSource::default();
                Ok(VideoSourceInner::Native(video_source))
            }
            _ => Err(FFIError::InvalidRequest("unsupported video source type")),
        }?;

        let video_source = Self {
            handle_id: server.next_id(),
            source_type,
            source: source_inner,
        };
        let source_info = proto::VideoSourceInfo::from(&video_source);

        server
            .ffi_handles()
            .write()
            .insert(video_source.handle_id, Box::new(video_source));

        Ok(source_info)
    }

    pub fn capture_frame(
        &self,
        server: &'static server::FFIServer,
        capture: proto::CaptureVideoFrameRequest,
    ) -> FFIResult<()> {
        match self.source {
            VideoSourceInner::Native(ref source) => {
                let frame_info = capture
                    .frame
                    .ok_or(FFIError::InvalidRequest("frame is empty"))?;

                let buffer_info = capture
                    .buffer
                    .ok_or(FFIError::InvalidRequest("buffer is none"))?;

                let ffi_handles = server.ffi_handles().read();
                let handle_id = buffer_info
                    .handle
                    .ok_or(FFIError::InvalidRequest("handle is empty"))?
                    .id as FFIHandleId;

                let buffer = ffi_handles
                    .get(&handle_id)
                    .ok_or(FFIError::InvalidRequest("handle not found"))?
                    .downcast_ref::<BoxVideoFrameBuffer>()
                    .ok_or(FFIError::InvalidRequest("handle is not video frame"))?;

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

    pub fn handle_id(&self) -> FFIHandleId {
        self.handle_id
    }

    pub fn source_type(&self) -> proto::VideoSourceType {
        self.source_type
    }
}
