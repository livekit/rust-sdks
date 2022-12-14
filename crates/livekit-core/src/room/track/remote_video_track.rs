use livekit_webrtc::media_stream::{MediaStreamTrackHandle, VideoTrack};
use std::sync::Arc;

use crate::room::track::{impl_track_trait, TrackShared};

#[derive(Debug)]
pub struct RemoteVideoTrack {
    shared: TrackShared,
}

impl RemoteVideoTrack {
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

impl_track_trait!(RemoteVideoTrack);
