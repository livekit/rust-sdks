use crate::proto;
use crate::{FfiAsyncId, FfiHandleId};

pub mod audio_frame;
pub mod participant;
pub mod publication;
pub mod room;
pub mod track;
pub mod video_frame;

impl From<FfiHandleId> for proto::FfiHandleId {
    fn from(id: FfiHandleId) -> Self {
        Self { id: id as u64 }
    }
}

impl From<FfiAsyncId> for proto::FfiAsyncId {
    fn from(id: FfiAsyncId) -> Self {
        Self { id: id as u64 }
    }
}
