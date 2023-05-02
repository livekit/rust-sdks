use crate::{proto, server, FFIError, FFIHandleId, FFIResult};
use futures_util::StreamExt;
use livekit::prelude::*;
use livekit::webrtc::media_stream::MediaStreamTrack;
use livekit::webrtc::video_frame::{BoxVideoFrameBuffer, VideoFrame};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use log::warn;
use tokio::sync::oneshot;

// ===== FFIVideoStream =====

pub struct FFIVideoStream {
    handle_id: FFIHandleId,
    stream_type: proto::VideoStreamType,
    track_sid: TrackSid,

    // When the sender is dropped, the stream will be closed
    #[allow(dead_code)]
    close_tx: oneshot::Sender<()>,
}

impl FFIVideoStream {
    pub fn setup(
        server: &'static server::FFIServer,
        new_stream: proto::NewVideoStreamRequest,
    ) -> FFIResult<proto::VideoStreamInfo> {
        let rooms = server.rooms.read();
        let session = rooms
            .get(&new_stream.room_sid.into())
            .ok_or(FFIError::InvalidRequest("room not found".to_string()))?
            .session();

        let participants = session.participants();
        let participant = participants.get(&new_stream.participant_sid.into()).ok_or(
            FFIError::InvalidRequest("participant not found".to_string()),
        )?;

        let tracks = participant.tracks();
        let track_sid = new_stream.track_sid.into();
        let track = tracks.get(&track_sid).ok_or(FFIError::InvalidRequest(
            "publication not found".to_string(),
        ))?;

        let track = track.track().ok_or(FFIError::InvalidRequest(
            "track not found/subscribed?".to_string(),
        ))?;

        let track = track.rtc_track();
        let MediaStreamTrack::Video(track) = track else {
            return Err(FFIError::InvalidRequest(
                "track is not a video track".to_string(),
            ));
        };

        let (close_tx, close_rx) = oneshot::channel();
        let stream_type = proto::VideoStreamType::from_i32(new_stream.r#type).unwrap();

        // TODO(theomonnom): other stream types (WebGL textures, + HTML)
        let stream = match stream_type {
            proto::VideoStreamType::VideoStreamNative => {
                let handle_id = server.next_id();
                tokio::spawn(native_video_stream_task(
                    server,
                    handle_id,
                    NativeVideoStream::new(track),
                    close_rx,
                ));
                Ok(Self {
                    handle_id,
                    close_tx,
                    stream_type,
                    track_sid,
                })
            }
            _ => {
                return Err(FFIError::InvalidRequest(
                    "video stream type is not supported".to_owned(),
                ))
            }
        }?;

        let info = proto::VideoStreamInfo::from(&stream);
        server
            .ffi_handles()
            .write()
            .insert(stream.handle_id, Box::new(stream));

        Ok(info)
    }
}

// Forward video frames to the foreign language
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
                        handle: Some(proto::FfiHandleId { id: stream_handle_id as u64 }),
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

impl From<&FFIVideoStream> for proto::VideoStreamInfo {
    fn from(stream: &FFIVideoStream) -> Self {
        Self {
            handle: Some(proto::FfiHandleId {
                id: stream.handle_id as u64,
            }),
            track_sid: stream.track_sid.clone().into(),
            r#type: stream.stream_type as i32,
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

        let source = match source_type {
            proto::VideoSourceType::VideoSourceNative => {
                let source = NativeVideoSource::default();
                Ok(VideoSourceInner::Native(source))
            }
            _ => Err(FFIError::InvalidRequest(
                "video source type is not supported".to_owned(),
            )),
        }?;

        let handle_id = server.next_id();
        let source = Self {
            handle_id,
            source_type,
            source,
        };
        let source_info = proto::VideoSourceInfo::from(&source);

        server
            .ffi_handles()
            .write()
            .insert(handle_id, Box::new(source));

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
                    .ok_or(FFIError::InvalidRequest("frame is none".to_owned()))?;

                let buffer_info = capture
                    .buffer
                    .ok_or(FFIError::InvalidRequest("buffer is none".to_owned()))?;

                let mut ffi_handles = server.ffi_handles().read();
                let handle_id = buffer_info.handle.unwrap().id as FFIHandleId;
                let buffer = *ffi_handles
                    .remove(&handle_id)
                    .ok_or(FFIError::HandleNotFound)?
                    .downcast::<BoxVideoFrameBuffer>()
                    .map_err(|_| FFIError::InvalidHandle)?;

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
}

impl From<&FFIVideoSource> for proto::VideoSourceInfo {
    fn from(source: &FFIVideoSource) -> Self {
        Self {
            handle: Some(proto::FfiHandleId {
                id: source.handle_id as u64,
            }),
            r#type: source.source_type as i32,
        }
    }
}
