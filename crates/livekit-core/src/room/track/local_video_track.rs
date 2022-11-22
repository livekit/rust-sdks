use crate::room::track::{impl_track_trait, TrackShared};

pub struct LocalVideoTrack {
    shared: TrackShared,
}

impl_track_trait!(LocalVideoTrack);
