use crate::proto;
use crate::FFIHandleId;
use livekit::prelude::*;

pub mod audio_frame;
pub mod participant;
pub mod publication;
pub mod room;
pub mod video_frame;

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
                    publications: p.tracks().iter().map(|(_, p)| p.into()).collect(),
                }
            }
        }
    };
}

impl_participant_into!(&LocalParticipant);
impl_participant_into!(&RemoteParticipant);
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
                    width: p.dimension().0,
                    height: p.dimension().1,
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
                    opt_handle: None,
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
impl_track_into!(&Track);
impl_track_into!(&LocalTrack);
impl_track_into!(&RemoteTrack);

impl From<TrackKind> for proto::TrackKind {
    fn from(kind: TrackKind) -> Self {
        match kind {
            TrackKind::Audio => proto::TrackKind::KindAudio,
            TrackKind::Video => proto::TrackKind::KindVideo,
        }
    }
}

impl From<StreamState> for proto::StreamState {
    fn from(state: StreamState) -> Self {
        match state {
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

impl From<&RoomSession> for proto::RoomInfo {
    fn from(session: &RoomSession) -> Self {
        Self {
            sid: session.sid().into(),
            name: session.name(),
            metadata: session.metadata(),
            local_participant: Some((&session.local_participant()).into()),
            participants: session
                .participants()
                .iter()
                .map(|(_, p)| p.into())
                .collect(),
        }
    }
}
