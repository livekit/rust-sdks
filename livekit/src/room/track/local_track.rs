use super::TrackInner;
use super::{track_dispatch, LocalAudioTrack, LocalVideoTrack};
use crate::prelude::*;
use crate::track::TrackEvent;
use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use livekit_webrtc::prelude::*;
use std::sync::Arc;
use tokio::sync::mpsc;

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

    #[inline]
    pub fn rtc_track(&self) -> MediaStreamTrack {
        match self {
            Self::Audio(track) => track.rtc_track().into(),
            Self::Video(track) => track.rtc_track().into(),
        }
    }
}
