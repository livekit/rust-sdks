use super::track_dispatch;
use crate::prelude::*;
use livekit_protocol::enum_dispatch;
use livekit_webrtc::prelude::*;

#[derive(Clone, Debug)]
pub enum AudioTrack {
    Local(LocalAudioTrack),
    Remote(RemoteAudioTrack),
}

impl AudioTrack {
    track_dispatch!([Local, Remote]);

    pub fn rtc_track(&self) -> RtcAudioTrack {
        match self {
            Self::Local(track) => track.rtc_track().into(),
            Self::Remote(track) => track.rtc_track().into(),
        }
    }
}

impl From<AudioTrack> for Track {
    fn from(track: AudioTrack) -> Self {
        match track {
            AudioTrack::Local(track) => Self::LocalAudio(track),
            AudioTrack::Remote(track) => Self::RemoteAudio(track),
        }
    }
}

impl TryFrom<Track> for AudioTrack {
    type Error = &'static str;

    fn try_from(track: Track) -> Result<Self, Self::Error> {
        match track {
            Track::LocalAudio(track) => Ok(Self::Local(track)),
            Track::RemoteAudio(track) => Ok(Self::Remote(track)),
            _ => Err("not an audio track"),
        }
    }
}
