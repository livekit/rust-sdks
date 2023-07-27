use super::{room, FfiHandle};
use crate::FfiHandleId;
use livekit::prelude::*;

#[derive(Clone)]
pub struct FfiParticipant {
    handle: FfiHandleId,
    participant: Participant,
    room: room::FfiRoom,
}

impl FfiHandle for FfiParticipant {}

impl FfiParticipant {
    pub fn handle(&self) -> FfiHandleId {
        self.handle
    }

    pub fn participant(&self) -> &Participant {
        &self.participant
    }

    pub fn room(&self) -> &room::FfiRoom {
        &self.room
    }
}
