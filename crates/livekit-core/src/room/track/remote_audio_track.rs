use crate::room::track::{impl_track_trait, TrackShared};
use livekit_webrtc::media_stream::{AudioTrack, MediaStreamTrackHandle};
use std::sync::Arc;

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
