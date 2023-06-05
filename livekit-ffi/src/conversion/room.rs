use crate::{proto, FfiHandleId, INVALID_HANDLE};
use livekit::options::{AudioEncoding, TrackPublishOptions, VideoEncoding};
use livekit::prelude::*;

impl proto::RoomEvent {
    pub fn from(room_handle: FfiHandleId, event: RoomEvent) -> Option<Self> {
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
                    track: Some(proto::TrackInfo::from_remote_track(INVALID_HANDLE, &track)),
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
            room_handle: Some(room_handle.into()),
            message: Some(message),
        })
    }
}

impl proto::RoomInfo {
    pub fn from_room(handle_id: FfiHandleId, session: &Room) -> Self {
        Self {
            handle: Some(handle_id.into()),
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

impl From<proto::TrackPublishOptions> for TrackPublishOptions {
    fn from(opts: proto::TrackPublishOptions) -> Self {
        Self {
            video_encoding: opts.video_encoding.map(Into::into),
            audio_encoding: opts.audio_encoding.map(Into::into),
            video_codec: proto::VideoCodec::from_i32(opts.video_codec)
                .unwrap()
                .into(),
            dtx: opts.dtx,
            red: opts.red,
            simulcast: opts.simulcast,
            name: opts.name,
            source: proto::TrackSource::from_i32(opts.source).unwrap().into(),
        }
    }
}

impl From<proto::VideoEncoding> for VideoEncoding {
    fn from(opts: proto::VideoEncoding) -> Self {
        Self {
            max_bitrate: opts.max_bitrate,
            max_framerate: opts.max_framerate,
        }
    }
}

impl From<proto::AudioEncoding> for AudioEncoding {
    fn from(opts: proto::AudioEncoding) -> Self {
        Self {
            max_bitrate: opts.max_bitrate,
        }
    }
}
