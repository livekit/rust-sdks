use std::sync::Arc;
use crate::room::participant::{impl_participant_trait, ParticipantShared};
use crate::room::track_publication::RemoteTrackPublication;

pub struct RemoteParticipant {
    shared: ParticipantShared,
}

impl RemoteParticipant {
    pub(super) fn new(info: ParticipantInfo) -> Self {
        Self {
            shared: ParticipantShared::new(
                info.sid.into(),
                info.identity.into(),
                info.name,
                info.metadata,
            ),
        }
    }

    pub(super) async fn add_subscribed_media_track() {


    }

    fn get_track_publication(&self, sid: &str) -> Option<RemoteTrackPublication> {
        let track = self.shared.tracks.read().get(&sid.to_string().into()).unwrap().clone();
        
        
        None
    }
}

impl_participant_trait!(RemoteParticipant);
