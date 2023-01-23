use crate::prelude::*;
use livekit_utils::enum_dispatch;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub enum RemoteTrackHandle {
    Audio(Arc<RemoteAudioTrack>),
    Video(Arc<RemoteVideoTrack>),
}

impl TrackTrait for RemoteTrackHandle {
    enum_dispatch!(
        [Audio, Video]
        fnc!(sid, &Self, [], TrackSid);
        fnc!(name, &Self, [], String);
        fnc!(kind, &Self, [], TrackKind);
        fnc!(stream_state, &Self, [], StreamState);
        fnc!(muted, &Self, [], bool);
        fnc!(start, &Self, [], ());
        fnc!(stop, &Self, [], ());
        fnc!(register_observer, &Self, [], mpsc::UnboundedReceiver<TrackEvent>);
        fnc!(set_muted, &Self, [muted: bool], ());
    );
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
