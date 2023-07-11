use crate::{proto, FfiHandleId};
use livekit::options::{AudioEncoding, TrackPublishOptions, VideoEncoding};
use livekit::prelude::*;

impl From<proto::RoomOptions> for RoomOptions {
    fn from(value: proto::RoomOptions) -> Self {
        Self {
            adaptive_stream: value.adaptive_stream,
            auto_subscribe: value.auto_subscribe,
            dynacast: value.dynacast,
        }
    }
}

impl From<proto::DataPacketKind> for DataPacketKind {
    fn from(value: proto::DataPacketKind) -> Self {
        match value {
            proto::DataPacketKind::KindReliable => Self::Reliable,
            proto::DataPacketKind::KindLossy => Self::Lossy,
        }
    }
}

impl From<DataPacketKind> for proto::DataPacketKind {
    fn from(value: DataPacketKind) -> Self {
        match value {
            DataPacketKind::Reliable => Self::KindReliable,
            DataPacketKind::Lossy => Self::KindLossy,
        }
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
