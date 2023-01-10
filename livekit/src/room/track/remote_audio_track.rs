use crate::prelude::*;
use crate::track::{impl_track_trait, TrackShared};
use std::sync::Arc;

#[derive(Debug)]
pub struct RemoteAudioTrack {
    shared: TrackShared,
}

impl RemoteAudioTrack {
    pub(crate) fn new(sid: TrackSid, name: String, track: Arc<AudioTrack>) -> Self {
        Self {
            shared: TrackShared::new(
                sid,
                name,
                TrackKind::Audio,
                MediaStreamTrackHandle::Audio(track),
            ),
        }
    }

    pub fn rtc_track(&self) -> Arc<AudioTrack> {
        if let MediaStreamTrackHandle::Audio(audio) = &self.shared.rtc_track {
            audio.clone()
        } else {
            unreachable!()
        }
    }
}

impl_track_trait!(RemoteAudioTrack);
