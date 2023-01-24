use super::impl_track_trait;
use crate::prelude::*;
use std::sync::Arc;

#[derive(Clone)]
pub enum VideoTrackHandle {
    Local(Arc<LocalVideoTrack>),
    Remote(Arc<RemoteVideoTrack>),
}

impl From<VideoTrackHandle> for TrackHandle {
    fn from(video_track: VideoTrackHandle) -> Self {
        match video_track {
            VideoTrackHandle::Local(local_video) => Self::LocalVideo(local_video),
            VideoTrackHandle::Remote(remote_video) => Self::RemoteVideo(remote_video),
        }
    }
}

impl TryFrom<TrackHandle> for VideoTrackHandle {
    type Error = &'static str;

    fn try_from(track: TrackHandle) -> Result<Self, Self::Error> {
        match track {
            TrackHandle::LocalVideo(local_video) => Ok(Self::Local(local_video)),
            TrackHandle::RemoteVideo(remote_video) => Ok(Self::Remote(remote_video)),
            _ => Err("not a video track"),
        }
    }
}

impl_track_trait!(VideoTrackHandle, enum_dispatch, [Local, Remote]);
