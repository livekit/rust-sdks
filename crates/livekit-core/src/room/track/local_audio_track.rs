use crate::room::track::{impl_track_trait, TrackShared};

pub struct LocalAudioTrack {
    shared: TrackShared,
}

impl_track_trait!(LocalAudioTrack);
