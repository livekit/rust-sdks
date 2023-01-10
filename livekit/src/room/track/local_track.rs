use crate::prelude::*;
use std::sync::Arc;

#[derive(Clone)]
pub enum LocalTrackHandle {
    Audio(Arc<LocalAudioTrack>),
    Video(Arc<LocalVideoTrack>),
}

impl From<LocalTrackHandle> for TrackHandle {
    fn from(local_track: LocalTrackHandle) -> Self {
        match local_track {
            LocalTrackHandle::Audio(local_audio) => Self::LocalAudio(local_audio),
            LocalTrackHandle::Video(local_video) => Self::LocalVideo(local_video),
        }
    }
}

impl TryFrom<TrackHandle> for LocalTrackHandle {
    type Error = &'static str;

    fn try_from(track: TrackHandle) -> Result<Self, Self::Error> {
        match track {
            TrackHandle::LocalAudio(local_audio) => Ok(Self::Audio(local_audio)),
            TrackHandle::LocalVideo(local_video) => Ok(Self::Video(local_video)),
            _ => Err("not a local track"),
        }
    }
}
