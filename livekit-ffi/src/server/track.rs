use super::FfiHandle;
use crate::FfiHandleId;
use livekit::prelude::*;

#[derive(Clone)]
pub struct FfiTrack {
    handle: FfiHandleId,
    track: Track,
}

impl FfiHandle for FfiTrack {}

impl FfiTrack {
    pub fn handle(&self) -> FfiHandleId {
        self.handle
    }

    pub fn track(&self) -> &Track {
        &self.track
    }
}
