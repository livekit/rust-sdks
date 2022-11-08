use crate::proto::ParticipantInfo;
use crate::room::local_participant::LocalParticipant;
use crate::room::remote_participant::RemoteParticipant;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::Arc;

pub(super) struct ParticipantShared {
    pub(super) sid: Mutex<ParticipantSid>,
    pub(super) identity: Mutex<ParticipantIdentity>,
    pub(super) name: Mutex<String>,
    pub(super) metadata: Mutex<String>,
    pub(super) tracks: RwLock<HashMap<TrackSid, TrackPublication>>,
}

impl ParticipantShared {
    pub(super) fn new(
        sid: ParticipantSid,
        identity: ParticipantIdentity,
        name: String,
        metadata: String,
    ) -> Self {
        Self {
            sid: Mutex::new(sid),
            identity: Mutex::new(identity),
            name: Mutex::new(name),
            metadata: Mutex::new(metadata),
            tracks: Default::default(),
        }
    }

    pub(crate) fn update_info(&self, info: ParticipantInfo) {
        *self.sid.lock() = info.sid.into();
        *self.identity.lock() = info.identity.into();
        *self.name.lock() = info.name;
        *self.metadata.lock() = info.metadata; // TODO(theomonnom): callback
    }
}

pub trait ParticipantTrait {
    fn sid(&self) -> ParticipantSid;
    fn identity(&self) -> ParticipantIdentity;
    fn name(&self) -> String;
    fn metadata(&self) -> String;
    fn update_info(&self, info: ParticipantInfo);
}

pub enum Participant {
    Local(LocalParticipant),
    Remote(RemoteParticipant),
}

macro_rules! shared_getter {
    ($x:ident, $ret:ident) => {
        fn $x(&self) -> $ret {
            match self {
                Participant::Local(p) => p.$x(),
                Participant::Remote(p) => p.$x(),
            }
        }
    };
}

impl ParticipantTrait for Participant {
    shared_getter!(sid, ParticipantSid);
    shared_getter!(identity, ParticipantIdentity);
    shared_getter!(name, String);
    shared_getter!(metadata, String);

    fn update_info(&self, info: ParticipantInfo) {
        match self {
            Participant::Local(p) => p.update_info(info),
            Participant::Remote(p) => p.update_info(info),
        }
    }
}

macro_rules! impl_participant_trait {
    ($x:ident) => {
        use crate::proto::ParticipantInfo;
        use crate::room::id::{ParticipantIdentity, ParticipantSid};

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

            fn update_info(&self, info: ParticipantInfo) {
                self.shared.update_info(info);
            }
        }
    };
}

use crate::room::id::{ParticipantIdentity, ParticipantSid, TrackSid};
use crate::room::track_publication::TrackPublication;
pub(super) use impl_participant_trait;
