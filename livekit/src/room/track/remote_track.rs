use super::track_dispatch;
use super::TrackInner;
use crate::prelude::*;
use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use livekit_webrtc::prelude::*;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum RemoteTrack {
    Audio(RemoteAudioTrack),
    Video(RemoteVideoTrack),
}

impl RemoteTrack {
    track_dispatch!([Audio, Video]);

    #[inline]
    pub fn rtc_track(&self) -> MediaStreamTrack {
        match self {
            Self::Audio(track) => track.rtc_track().into(),
            Self::Video(track) => track.rtc_track().into(),
        }
    }
}

pub(crate) fn update_info(track: &Arc<TrackInner>, new_info: proto::TrackInfo) {
    track.update_info(new_info.clone());
    track.set_muted(new_info.muted);
}

impl From<RemoteTrack> for Track {
    fn from(track: RemoteTrack) -> Self {
        match track {
            RemoteTrack::Audio(track) => Self::RemoteAudio(track),
            RemoteTrack::Video(track) => Self::RemoteVideo(track),
        }
    }
}

impl TryFrom<Track> for RemoteTrack {
    type Error = &'static str;

    fn try_from(track: Track) -> Result<Self, Self::Error> {
        match track {
            Track::RemoteAudio(track) => Ok(Self::Audio(track)),
            Track::RemoteVideo(track) => Ok(Self::Video(track)),
            _ => Err("not a local track"),
        }
    }
}
