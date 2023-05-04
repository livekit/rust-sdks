use crate::proto;
use livekit::prelude::*;

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
                    track: Some(proto::TrackInfo::from_remote_track(None, &track)),
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
