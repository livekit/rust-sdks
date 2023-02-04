use crate::{proto, server::FFIHandleId};
use livekit::{
    prelude::*,
    webrtc::video_frame_buffer::{
        BiplanarYuv8Buffer, BiplanarYuvBuffer, I010Buffer, I420ABuffer, I420Buffer, I422Buffer,
        I444Buffer, NV12Buffer, PlanarYuv16BBuffer, PlanarYuv8Buffer, PlanarYuvBuffer,
    },
};
use std::sync::Arc;

impl From<FFIHandleId> for proto::FfiHandleId {
    fn from(id: FFIHandleId) -> Self {
        Self { id: id as u64 }
    }
}

macro_rules! impl_participant_into {
    ($p:ty) => {
        impl From<$p> for proto::ParticipantInfo {
            fn from(p: $p) -> Self {
                Self {
                    name: p.name(),
                    sid: p.sid().to_string(),
                    identity: p.identity().to_string(),
                    metadata: p.metadata(),
                }
            }
        }
    };
}

impl_participant_into!(&Arc<LocalParticipant>);
impl_participant_into!(&Arc<RemoteParticipant>);
impl_participant_into!(&Participant);

impl From<TrackSource> for proto::TrackSource {
    fn from(source: TrackSource) -> proto::TrackSource {
        match source {
            TrackSource::Unknown => proto::TrackSource::SourceUnknown,
            TrackSource::Camera => proto::TrackSource::SourceCamera,
            TrackSource::Microphone => proto::TrackSource::SourceMicrophone,
            TrackSource::Screenshare => proto::TrackSource::SourceScreenshare,
            TrackSource::ScreenshareAudio => proto::TrackSource::SourceScreenshareAudio,
        }
    }
}

macro_rules! impl_publication_into {
    ($p:ty) => {
        impl From<$p> for proto::TrackPublicationInfo {
            fn from(p: $p) -> Self {
                Self {
                    name: p.name(),
                    sid: p.sid().to_string(),
                    kind: proto::TrackKind::from(p.kind()).into(),
                    source: proto::TrackSource::from(p.source()).into(),
                    dimension: Some(proto::Dimension {
                        width: p.dimension().0,
                        height: p.dimension().1,
                    }),
                    mime_type: p.mime_type(),
                    simulcasted: p.simulcasted(),
                    muted: p.muted(),
                }
            }
        }
    };
}

impl_publication_into!(&LocalTrackPublication);
impl_publication_into!(&RemoteTrackPublication);
impl_publication_into!(&TrackPublication);

macro_rules! impl_track_into {
    ($t:ty) => {
        impl From<$t> for proto::TrackInfo {
            fn from(track: $t) -> Self {
                Self {
                    name: track.name(),
                    stream_state: proto::StreamState::from(track.stream_state()).into(),
                    sid: track.sid().to_string(),
                    kind: proto::TrackKind::from(track.kind()).into(),
                    muted: track.muted(),
                }
            }
        }
    };
}

impl_track_into!(&LocalAudioTrack);
impl_track_into!(&LocalVideoTrack);
impl_track_into!(&RemoteAudioTrack);
impl_track_into!(&RemoteVideoTrack);
impl_track_into!(&TrackHandle);
impl_track_into!(&LocalTrackHandle);
impl_track_into!(&RemoteTrackHandle);

impl From<TrackKind> for proto::TrackKind {
    fn from(kind: TrackKind) -> Self {
        match kind {
            TrackKind::Unknown => proto::TrackKind::KindUnknown,
            TrackKind::Audio => proto::TrackKind::KindAudio,
            TrackKind::Video => proto::TrackKind::KindVideo,
        }
    }
}

impl From<StreamState> for proto::StreamState {
    fn from(state: StreamState) -> Self {
        match state {
            StreamState::Unknown => Self::StateUnknown,
            StreamState::Active => Self::StateActive,
            StreamState::Paused => Self::StatePaused,
        }
    }
}

impl proto::RoomEvent {
    pub fn from(room_sid: impl Into<String>, event: RoomEvent) -> Option<Self> {
        let message = match event {
            RoomEvent::ParticipantConnected(participant) => Some(
                proto::room_event::Message::ParticipantConnected(proto::ParticipantConnected {
                    info: Some((&participant).into()),
                }),
            ),
            RoomEvent::ParticipantDisconnected(participant) => {
                Some(proto::room_event::Message::ParticipantDisconnected(
                    proto::ParticipantDisconnected {
                        info: Some((&participant).into()),
                    },
                ))
            }
            RoomEvent::TrackPublished {
                publication,
                participant,
            } => Some(proto::room_event::Message::TrackPublished(
                proto::TrackPublished {
                    participant_sid: participant.sid().to_string(),
                    publication: Some((&publication).into()),
                },
            )),
            RoomEvent::TrackUnpublished {
                publication,
                participant,
            } => Some(proto::room_event::Message::TrackUnpublished(
                proto::TrackUnpublished {
                    participant_sid: participant.sid().to_string(),
                    publication_sid: publication.sid().into(),
                },
            )),
            RoomEvent::TrackSubscribed {
                track,
                publication: _,
                participant,
            } => Some(proto::room_event::Message::TrackSubscribed(
                proto::TrackSubscribed {
                    participant_sid: participant.sid().to_string(),
                    track: Some((&track).into()),
                    sink: Some(proto::VideoSinkInfo {
                        track_sid: track.sid().to_string(),
                    }),
                },
            )),
            RoomEvent::TrackUnsubscribed {
                track,
                publication: _,
                participant,
            } => Some(proto::room_event::Message::TrackUnsubscribed(
                proto::TrackUnsubscribed {
                    participant_sid: participant.sid().to_string(),
                    track_sid: track.sid().to_string(),
                },
            )),
            _ => None,
        };

        message.map(|message| proto::RoomEvent {
            room_sid: room_sid.into(),
            message: Some(message),
        })
    }
}

impl From<VideoRotation> for proto::VideoRotation {
    fn from(rotation: VideoRotation) -> proto::VideoRotation {
        match rotation {
            VideoRotation::VideoRotation0 => Self::VideoRotation0,
            VideoRotation::VideoRotation90 => Self::VideoRotation90,
            VideoRotation::VideoRotation180 => Self::VideoRotation180,
            VideoRotation::VideoRotation270 => Self::VideoRotation270,
        }
    }
}

impl From<VideoFrame> for proto::VideoFrameInfo {
    fn from(frame: VideoFrame) -> Self {
        Self {
            width: frame.width(),
            height: frame.height(),
            size: frame.size(),
            id: frame.id() as u32,
            timestamp_us: frame.timestamp_us(),
            ntp_time_ms: frame.ntp_time_ms(),
            transport_frame_id: frame.transport_frame_id(),
            timestamp: frame.timestamp(),
            rotation: proto::VideoRotation::from(frame.rotation()).into(),
        }
    }
}

impl From<VideoFrameBufferType> for proto::VideoFrameBufferType {
    fn from(buffer_type: VideoFrameBufferType) -> Self {
        match buffer_type {
            VideoFrameBufferType::Native => Self::Native,
            VideoFrameBufferType::I420 => Self::I420,
            VideoFrameBufferType::I420A => Self::I420a,
            VideoFrameBufferType::I422 => Self::I422,
            VideoFrameBufferType::I444 => Self::I444,
            VideoFrameBufferType::I010 => Self::I010,
            VideoFrameBufferType::NV12 => Self::Nv12,
        }
    }
}

macro_rules! impl_yuv_into {
    ($b:ty) => {
        impl From<$b> for proto::PlanarYuvBufferInfo {
            fn from(buffer: $b) -> Self {
                Self {
                    chroma_width: buffer.chroma_width(),
                    chroma_height: buffer.chroma_height(),
                    stride_y: buffer.stride_y(),
                    stride_u: buffer.stride_u(),
                    stride_v: buffer.stride_v(),
                    data_y_ptr: buffer.data_y().as_ptr() as u64,
                    data_u_ptr: buffer.data_u().as_ptr() as u64,
                    data_v_ptr: buffer.data_v().as_ptr() as u64,
                }
            }
        }
    };
}

impl_yuv_into!(&I420Buffer);
impl_yuv_into!(&I420ABuffer);
impl_yuv_into!(&I422Buffer);
impl_yuv_into!(&I444Buffer);
impl_yuv_into!(&I010Buffer);

macro_rules! impl_biyuv_into {
    ($b:ty) => {
        impl From<$b> for proto::BiplanarYuvBufferInfo {
            fn from(buffer: $b) -> Self {
                Self {
                    chroma_width: buffer.chroma_width(),
                    chroma_height: buffer.chroma_height(),
                    stride_y: buffer.stride_y(),
                    stride_uv: buffer.stride_uv(),
                    data_y_ptr: buffer.data_y().as_ptr() as u64,
                    data_uv_ptr: buffer.data_uv().as_ptr() as u64,
                }
            }
        }
    };
}

impl_biyuv_into!(&NV12Buffer);

impl proto::VideoFrameBufferInfo {
    pub fn from(handle_id: FFIHandleId, buffer: &VideoFrameBuffer) -> Self {
        Self {
            handle: Some(handle_id.into()),
            buffer_type: proto::VideoFrameBufferType::from(buffer.buffer_type()).into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(match &buffer {
                VideoFrameBuffer::Native(_) => {
                    proto::video_frame_buffer_info::Buffer::Native(proto::NativeBufferInfo {})
                }
                VideoFrameBuffer::I420(i420) => {
                    proto::video_frame_buffer_info::Buffer::Yuv(i420.into())
                }
                VideoFrameBuffer::I420A(i420a) => {
                    proto::video_frame_buffer_info::Buffer::Yuv(i420a.into())
                }
                VideoFrameBuffer::I422(i422) => {
                    proto::video_frame_buffer_info::Buffer::Yuv(i422.into())
                }
                VideoFrameBuffer::I444(i444) => {
                    proto::video_frame_buffer_info::Buffer::Yuv(i444.into())
                }
                VideoFrameBuffer::I010(i010) => {
                    proto::video_frame_buffer_info::Buffer::Yuv(i010.into())
                }
                VideoFrameBuffer::NV12(nv12) => {
                    proto::video_frame_buffer_info::Buffer::BiYuv(nv12.into())
                }
            }),
        }
    }
}

impl From<proto::VideoFormatType> for VideoFormatType {
    fn from(format: proto::VideoFormatType) -> Self {
        match format {
            proto::VideoFormatType::FormatArgb => Self::ARGB,
            proto::VideoFormatType::FormatBgra => Self::BGRA,
            proto::VideoFormatType::FormatAbgr => Self::ABGR,
            proto::VideoFormatType::FormatRgba => Self::RGBA,
        }
    }
}
