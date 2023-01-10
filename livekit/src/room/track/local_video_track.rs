use super::{impl_track_trait, TrackShared};

#[derive(Debug)]
pub struct LocalVideoTrack {
    shared: TrackShared,
}

impl_track_trait!(LocalVideoTrack);
