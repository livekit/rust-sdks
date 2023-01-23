use crate::prelude::*;
use livekit_utils::enum_dispatch;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Clone)]
pub enum LocalTrackHandle {
    Audio(Arc<LocalAudioTrack>),
    Video(Arc<LocalVideoTrack>),
}

impl TrackTrait for LocalTrackHandle {
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
