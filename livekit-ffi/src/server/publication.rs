use crate::FfiHandleId;
use livekit::prelude::*;

use super::FfiHandle;

#[derive(Clone)]
pub struct FfiPublication {
    handle: FfiHandleId,
    publication: TrackPublication,
}

impl FfiHandle for FfiPublication {}

impl FfiPublication {
    pub fn handle(&self) -> FfiHandleId {
        self.handle
    }

    pub fn publication(&self) -> &TrackPublication {
        &self.publication
    }
}
