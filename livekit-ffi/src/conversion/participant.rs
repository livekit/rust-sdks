use crate::proto;
use livekit::prelude::*;

macro_rules! impl_participant_into {
    ($p:ty) => {
        impl From<$p> for proto::ParticipantInfo {
            fn from(p: $p) -> Self {
                Self {
                    name: p.name(),
                    sid: p.sid().to_string(),
                    identity: p.identity().to_string(),
                    metadata: p.metadata(),
                }
            }
        }
    };
}

impl_participant_into!(&LocalParticipant);
impl_participant_into!(&RemoteParticipant);
impl_participant_into!(&Participant);
