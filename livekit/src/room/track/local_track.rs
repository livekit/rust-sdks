use super::track_dispatch;
use crate::prelude::*;
use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use livekit_webrtc::prelude::*;

#[derive(Clone, Debug)]
pub enum LocalTrack {
    Audio(LocalAudioTrack),
    Video(LocalVideoTrack),
}

impl LocalTrack {
    track_dispatch!([Audio, Video]);

    enum_dispatch!(
       [Audio, Video];
        pub fn mute(self: &Self) -> ();
        pub fn unmute(self: &Self) -> ();
    );

    pub fn rtc_track(&self) -> MediaStreamTrack {
        match self {
            Self::Audio(track) => track.rtc_track().into(),
            Self::Video(track) => track.rtc_track().into(),
        }
    }
}

impl From<LocalTrack> for Track {
    fn from(track: LocalTrack) -> Self {
        match track {
            LocalTrack::Audio(track) => Self::LocalAudio(track),
            LocalTrack::Video(track) => Self::LocalVideo(track),
        }
    }
}

impl TryFrom<Track> for LocalTrack {
    type Error = &'static str;

    fn try_from(track: Track) -> Result<Self, Self::Error> {
        match track {
            Track::LocalAudio(track) => Ok(Self::Audio(track)),
            Track::LocalVideo(track) => Ok(Self::Video(track)),
            _ => Err("not a local track"),
        }
    }
}
