use crate::events::participant::ParticipantEvents;
use crate::proto::ParticipantInfo;
use crate::room::id::{ParticipantIdentity, ParticipantSid, TrackSid};
use crate::room::participant::local_participant::LocalParticipant;
use crate::room::participant::remote_participant::RemoteParticipant;
use crate::room::publication::{TrackPublication, TrackPublicationTrait};
use crate::utils::wrap_variants;
use futures_util::future::BoxFuture;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::Arc;

pub mod local_participant;
pub mod remote_participant;

type OnTrackSubscribed = Box<dyn FnMut(ParticipantHandle) -> BoxFuture<'static, ()> + Send + Sync>;

pub(super) struct ParticipantShared {
    pub(super) events: Arc<ParticipantEvents>,
    pub(super) internal_events: Arc<ParticipantEvents>,
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
            events: Default::default(),
            internal_events: Default::default(),
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
        *self.metadata.lock() = info.metadata; // TODO(theomonnom): callback MetadataChanged
    }

    pub(crate) fn add_track_publication(&self, publication: TrackPublication) {
        self.tracks.write().insert(publication.sid(), publication);
    }
}

pub(crate) trait ParticipantInternalTrait {
    fn internal_events(&self) -> Arc<ParticipantEvents>;
}

pub trait ParticipantTrait {
    fn events(&self) -> Arc<ParticipantEvents>;
    fn sid(&self) -> ParticipantSid;
    fn identity(&self) -> ParticipantIdentity;
    fn name(&self) -> String;
    fn metadata(&self) -> String;
    fn update_info(&self, info: ParticipantInfo);
}

#[derive(Clone)]
pub enum ParticipantHandle {
    Local(Arc<LocalParticipant>),
    Remote(Arc<RemoteParticipant>),
}

impl ParticipantInternalTrait for ParticipantHandle {
    wrap_variants!(
        [Local, Remote]
        fnc!(internal_events, Arc<ParticipantEvents>, []);
    );
}

impl ParticipantTrait for ParticipantHandle {
    wrap_variants!(
        [Local, Remote]
        fnc!(events, Arc<ParticipantEvents>, []);
        fnc!(sid, ParticipantSid, []);
        fnc!(identity, ParticipantIdentity, []);
        fnc!(name, String, []);
        fnc!(metadata, String, []);
        fnc!(update_info, (), [info: ParticipantInfo]);
    );
}

macro_rules! impl_participant_trait {
    ($x:ty) => {
        use crate::events::participant::ParticipantEvents;
        use crate::proto::ParticipantInfo;
        use crate::room::id::{ParticipantIdentity, ParticipantSid};
        use std::sync::Arc;

        impl crate::room::participant::ParticipantInternalTrait for $x {
            fn internal_events(&self) -> Arc<ParticipantEvents> {
                self.shared.internal_events.clone()
            }
        }

        impl crate::room::participant::ParticipantTrait for $x {
            fn events(&self) -> Arc<ParticipantEvents> {
                self.shared.events.clone()
            }

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

pub(super) use impl_participant_trait;
