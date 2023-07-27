use crate::{proto, server::participant::FfiParticipant};

impl proto::ParticipantInfo {
    pub fn from(handle: proto::FfiOwnedHandle, participant: &FfiParticipant) -> Self {
        let participant = participant.participant();
        Self {
            handle: Some(handle),
            sid: participant.sid(),
            name: participant.name(),
            identity: participant.identity(),
            metadata: participant.metadata(),
        }
    }
}
