use crate::{proto, server, FFIError, FFIHandleId, FFIResult};
use futures_util::StreamExt;
use livekit::prelude::*;
use livekit::webrtc::media_stream::MediaStreamTrack;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use log::warn;
use tokio::sync::oneshot;
use uuid::Uuid;

// TODO(theomonnom): other stream types (WebGL textures, + HTML)

pub struct FFIVideoStream {
    id: String,
    stream_type: proto::VideoStreamType,
    track_sid: TrackSid,

    // When the sender is dropped, the stream will be closed
    #[allow(dead_code)]
    close_tx: oneshot::Sender<()>,
}

impl FFIVideoStream {
    pub fn from_request(
        server: &'static server::FFIServer,
        new_stream: proto::NewVideoStreamRequest,
    ) -> FFIResult<Self> {
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

        let id = Uuid::new_v4().to_string();
        let stream_type = proto::VideoStreamType::from_i32(new_stream.r#type).unwrap();
        match stream_type {
            proto::VideoStreamType::VideoStreamNative => {
                tokio::spawn(native_video_stream_task(
                    server,
                    id.clone(),
                    NativeVideoStream::new(track),
                    close_rx,
                ));
                Ok(Self {
                    id,
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
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

// Forward video frames to the foreign language
async fn native_video_stream_task(
    server: &'static server::FFIServer,
    id: String,
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
                        id: id.clone(),
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

impl proto::VideoStreamInfo {
    pub fn from(handle: FFIHandleId, stream: &FFIVideoStream) -> Self {
        Self {
            handle: Some(proto::FfiHandleId { id: handle as u64 }),
            id: stream.id.clone(),
            track_sid: stream.track_sid.clone().into(),
            r#type: stream.stream_type as i32,
        }
    }
}
