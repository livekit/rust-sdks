use crate::{proto, FfiHandleId};
use livekit::options::{AudioEncoding, TrackPublishOptions, VideoEncoding};
use livekit::prelude::*;

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
