use crate::{proto, server::room::FfiParticipant};

impl proto::ParticipantInfo {
    pub fn from(handle: proto::FfiOwnedHandle, ffi_participant: &FfiParticipant) -> Self {
        let participant = &ffi_participant.participant;
        Self {
            handle: Some(handle),
            sid: participant.sid(),
            name: participant.name(),
            identity: participant.identity(),
            metadata: participant.metadata(),
        }
    }
}
