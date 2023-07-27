use crate::FfiHandleId;
use livekit::prelude::*;

pub struct FfiParticipant {
    handle: FfiHandleId,
    participant: Participant,
}
