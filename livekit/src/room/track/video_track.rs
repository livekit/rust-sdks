use super::track_dispatch;
use crate::prelude::*;
use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use livekit_webrtc::prelude::*;

#[derive(Clone, Debug)]
pub enum VideoTrack {
    Local(LocalVideoTrack),
    Remote(RemoteVideoTrack),
}

impl VideoTrack {
    track_dispatch!([Local, Remote]);

    #[inline]
    pub fn rtc_track(&self) -> RtcVideoTrack {
        match self {
            Self::Local(track) => track.rtc_track(),
            Self::Remote(track) => track.rtc_track(),
        }
    }
}

impl From<VideoTrack> for Track {
    fn from(track: VideoTrack) -> Self {
        match track {
            VideoTrack::Local(track) => Self::LocalVideo(track),
            VideoTrack::Remote(track) => Self::RemoteVideo(track),
        }
    }
}

impl TryFrom<Track> for VideoTrack {
    type Error = &'static str;

    fn try_from(track: Track) -> Result<Self, Self::Error> {
        match track {
            Track::LocalVideo(track) => Ok(Self::Local(track)),
            Track::RemoteVideo(track) => Ok(Self::Remote(track)),
            _ => Err("not a video track"),
        }
    }
}
