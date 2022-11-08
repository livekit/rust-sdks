use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use parking_lot::Mutex;
use crate::room::id::{ParticipantIdentity, ParticipantSid, TrackSid};
use crate::room::track::{TrackKind, TrackSource};

pub trait TrackPublicationTrait {
    fn name(&self) -> String;
    fn sid(&self) -> TrackSid;
    fn kind(&self) -> TrackKind;
    fn source(&self) -> TrackSource;
    fn simulcasted(&self) -> bool;
}

pub(super) struct TrackPublicationShared {
    pub(super) name: Mutex<String>,
    pub(super) sid: Mutex<TrackSid>,
    pub(super) kind: AtomicU8, // Casted to TrackKind
    pub(super) source: AtomicU8, // Casted to TrackSource
    pub(super) simulcasted: AtomicBool
}

#[derive(Clone)]
pub enum TrackPublication {
    Local(LocalTrackPublication),
    Remote(RemoteTrackPublication)
}

macro_rules! shared_getter {
    ($x:ident, $ret:ident) => {
        fn $x(&self) -> $ret {
            match self {
                TrackPublication::Local(p) => p.$x(),
                TrackPublication::Remote(p) => p.$x(),
            }
        }
    };
}

impl TrackPublicationTrait for TrackPublication {
    shared_getter!(name, String);
    shared_getter!(sid, TrackSid);
    shared_getter!(kind, TrackKind);
    shared_getter!(source, TrackSource);
    shared_getter!(simulcasted, bool);
}

macro_rules! impl_publication_trait {
    ($x:ident) => {
        impl TrackPublicationTrait for $x {
            fn name(&self) -> String {
                self.shared.name.lock().clone()
            }

            fn sid(&self) -> TrackSid {
                self.shared.sid.lock().clone()
            }

            fn kind(&self) -> TrackKind {
                self.shared.kind.load(Ordering::SeqCst).into()
            }

            fn source(&self) -> TrackSource {
                self.shared.source.load(Ordering::SeqCst).into()
            }

            fn simulcasted(&self) -> bool {
                self.shared.simulcasted.load(Ordering::SeqCst)
            }
        }
    }
}



#[derive(Clone)]
pub struct LocalTrackPublication {
    shared: Arc<TrackPublicationShared>
}

#[derive(Clone)]
pub struct RemoteTrackPublication {
    shared: Arc<TrackPublicationShared>
}

impl_publication_trait!(LocalTrackPublication);
impl_publication_trait!(RemoteTrackPublication);
