use super::{impl_track_trait, TrackShared};
use crate::prelude::*;
use std::sync::Arc;

#[derive(Debug)]
pub struct LocalVideoTrack {
    shared: TrackShared,
}

impl LocalVideoTrack {
    pub(crate) fn new(sid: TrackSid, name: String, track: Arc<VideoTrack>) -> Self {
        Self {
            shared: TrackShared::new(
                sid,
                name,
                TrackKind::Video,
                MediaStreamTrackHandle::Video(track),
            ),
        }
    }

    pub fn rtc_track(&self) -> Arc<VideoTrack> {
        if let MediaStreamTrackHandle::Video(video) = &self.shared.rtc_track {
            video.clone()
        } else {
            unreachable!()
        }
    }
}

impl_track_trait!(LocalVideoTrack, VideoTrack);
