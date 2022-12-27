use crate::proto::ParticipantInfo;
use crate::room::id::{ParticipantIdentity, ParticipantSid, TrackSid};
use crate::room::participant::local_participant::LocalParticipant;
use crate::room::participant::remote_participant::RemoteParticipant;
use crate::room::publication::{TrackPublication, TrackPublicationTrait};
use livekit_utils::enum_dispatch;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::Arc;

pub mod local_participant;
pub mod remote_participant;

#[derive(Debug)]
pub(super) struct ParticipantShared {
    pub(super) sid: Mutex<ParticipantSid>,
    pub(super) identity: Mutex<ParticipantIdentity>,
    pub(super) name: Mutex<String>,
    pub(super) metadata: Mutex<String>,
    pub(super) tracks: RwLock<HashMap<TrackSid, TrackPublication>>,
    pub(super) room_emitter: RoomEmitter,
}

impl ParticipantShared {
    pub(super) fn new(
        sid: ParticipantSid,
        identity: ParticipantIdentity,
        name: String,
        metadata: String,
        room_emitter: RoomEmitter,
    ) -> Self {
        Self {
            sid: Mutex::new(sid),
            identity: Mutex::new(identity),
            name: Mutex::new(name),
            metadata: Mutex::new(metadata),
            tracks: Default::default(),
            room_emitter
        }
    }

    pub(crate) fn update_info(&self, info: ParticipantInfo) {
        *self.sid.lock() = info.sid.into();
        *self.identity.lock() = info.identity.into();
        *self.name.lock() = info.name;
        *self.metadata.lock() = info.metadata; // TODO(theomonnom): callback MetadataChanged
    }

    pub(crate) fn add_track_publication(&self, publication: TrackPublication) {
        self.tracks.write().insert(publication.sid(), publication);
    }
}

pub(crate) trait ParticipantInternalTrait {
    fn update_info(&self, info: ParticipantInfo);
}

pub trait ParticipantTrait {
    fn sid(&self) -> ParticipantSid;
    fn identity(&self) -> ParticipantIdentity;
    fn name(&self) -> String;
    fn metadata(&self) -> String;
}

#[derive(Clone)]
pub enum ParticipantHandle {
    Local(Arc<LocalParticipant>),
    Remote(Arc<RemoteParticipant>),
}

impl ParticipantInternalTrait for ParticipantHandle {
    enum_dispatch!(
        [Local, Remote]
        fnc!(update_info, &Self, [info: ParticipantInfo], ());
    );
}

impl ParticipantTrait for ParticipantHandle {
    enum_dispatch!(
        [Local, Remote]
        fnc!(sid, &Self, [], ParticipantSid);
        fnc!(identity, &Self, [], ParticipantIdentity);
        fnc!(name, &Self, [], String);
        fnc!(metadata, &Self, [], String);
    );
}

macro_rules! impl_participant_trait {
    ($x:ty) => {
        use crate::proto::ParticipantInfo;
        use crate::room::id::{ParticipantIdentity, ParticipantSid};
        use std::sync::Arc;

        impl crate::room::participant::ParticipantTrait for $x {
            fn sid(&self) -> ParticipantSid {
                self.shared.sid.lock().clone()
            }

            fn identity(&self) -> ParticipantIdentity {
                self.shared.identity.lock().clone()
            }

            fn name(&self) -> String {
                self.shared.name.lock().clone()
            }

            fn metadata(&self) -> String {
                self.shared.metadata.lock().clone()
            }
        }
    };
}

pub(super) use impl_participant_trait;

use super::RoomEmitter;
