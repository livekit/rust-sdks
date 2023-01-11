use crate::prelude::*;
use std::sync::Arc;

#[derive(Clone)]
pub enum AudioTrackHandle {
    Local(Arc<LocalAudioTrack>),
    Remote(Arc<RemoteAudioTrack>),
}

impl From<AudioTrackHandle> for TrackHandle {
    fn from(audio_track: AudioTrackHandle) -> Self {
        match audio_track {
            AudioTrackHandle::Local(local_audio) => Self::LocalAudio(local_audio),
            AudioTrackHandle::Remote(remote_audio) => Self::RemoteAudio(remote_audio),
        }
    }
}

impl TryFrom<TrackHandle> for AudioTrackHandle {
    type Error = &'static str;

    fn try_from(track: TrackHandle) -> Result<Self, Self::Error> {
        match track {
            TrackHandle::LocalAudio(local_audio) => Ok(Self::Local(local_audio)),
            TrackHandle::RemoteAudio(remote_audio) => Ok(Self::Remote(remote_audio)),
            _ => Err("not a audio track"),
        }
    }
}
