use super::impl_track_trait;
use crate::prelude::*;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum RemoteTrackHandle {
    Audio(Arc<RemoteAudioTrack>),
    Video(Arc<RemoteVideoTrack>),
}

impl From<RemoteTrackHandle> for TrackHandle {
    fn from(remote_track: RemoteTrackHandle) -> Self {
        match remote_track {
            RemoteTrackHandle::Audio(remote_audio) => Self::RemoteAudio(remote_audio),
            RemoteTrackHandle::Video(remote_video) => Self::RemoteVideo(remote_video),
        }
    }
}

impl TryFrom<TrackHandle> for RemoteTrackHandle {
    type Error = &'static str;

    fn try_from(track: TrackHandle) -> Result<Self, Self::Error> {
        match track {
            TrackHandle::RemoteAudio(remote_audio) => Ok(Self::Audio(remote_audio)),
            TrackHandle::RemoteVideo(remote_video) => Ok(Self::Video(remote_video)),
            _ => Err("not a remote track"),
        }
    }
}

impl_track_trait!(RemoteTrackHandle, enum_dispatch, [Audio, Video]);
