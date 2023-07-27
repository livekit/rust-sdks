use livekit::prelude::*;

use crate::FfiHandleId;

pub struct FfiTrack {
    handle: FfiHandleId,
    track: Track,
}

pub struct FfiPublication {
    handle: FfiHandleId,
    publication: TrackPublication,
}
