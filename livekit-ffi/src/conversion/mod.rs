use crate::proto;
use crate::{FFIAsyncId, FFIHandleId};

pub mod audio_frame;
pub mod participant;
pub mod publication;
pub mod room;
pub mod track;
pub mod video_frame;

impl From<FFIHandleId> for proto::FfiHandleId {
    fn from(id: FFIHandleId) -> Self {
        Self { id: id as u64 }
    }
}

impl From<FFIAsyncId> for proto::FfiAsyncId {
    fn from(id: FFIAsyncId) -> Self {
        Self { id: id as u64 }
    }
}
