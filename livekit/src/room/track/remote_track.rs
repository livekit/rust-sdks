use super::track_dispatch;
use super::TrackInner;
use super::{RemoteAudioTrack, RemoteVideoTrack};
use crate::prelude::*;
use crate::track::TrackEvent;
use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use livekit_webrtc::prelude::*;
use std::sync::Arc;
use tokio::sync::mpsc;

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
