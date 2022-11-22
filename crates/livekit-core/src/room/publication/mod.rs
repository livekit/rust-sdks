use crate::room::id::TrackSid;
use crate::room::track::local_track::LocalTrackHandle;
use crate::room::track::remote_track::RemoteTrackHandle;
use crate::room::track::{TrackHandle, TrackKind, TrackSource};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

pub trait TrackPublicationTrait {
    fn name(&self) -> String;
    fn sid(&self) -> TrackSid;
    fn kind(&self) -> TrackKind;
    fn source(&self) -> TrackSource;
    fn simulcasted(&self) -> bool;
}

pub(super) struct TrackPublicationShared {
    pub(super) track: Mutex<Option<TrackHandle>>,
    pub(super) name: Mutex<String>,
    pub(super) sid: Mutex<TrackSid>,
    pub(super) kind: AtomicU8,   // Casted to TrackKind
    pub(super) source: AtomicU8, // Casted to TrackSource
    pub(super) simulcasted: AtomicBool,
}

#[derive(Clone)]
pub enum TrackPublication {
    Local(LocalTrackPublication),
    Remote(RemoteTrackPublication),
}

macro_rules! shared_getter {
    ($x:ident, $ret:ty) => {
        fn $x(&self) -> $ret {
            match self {
                TrackPublication::Local(p) => p.$x(),
                TrackPublication::Remote(p) => p.$x(),
            }
        }
    };
}

impl TrackPublication {
    pub fn track(&self) -> Option<TrackHandle> {
        match self {
            TrackPublication::Local(p) => p.shared.track.lock().clone(),
            TrackPublication::Remote(p) => p.shared.track.lock().clone(),
        }
    }
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
    };
}

#[derive(Clone)]
pub struct LocalTrackPublication {
    shared: Arc<TrackPublicationShared>,
}

impl LocalTrackPublication {
    pub fn track(&self) -> Option<LocalTrackHandle> {
        self.shared
            .track
            .lock()
            .clone()
            .map(|local_track| local_track.try_into().unwrap())
    }
}

#[derive(Clone)]
pub struct RemoteTrackPublication {
    shared: Arc<TrackPublicationShared>,
}

impl RemoteTrackPublication {
    pub fn track(&self) -> Option<RemoteTrackHandle> {
        self.shared
            .track
            .lock()
            .clone()
            .map(|track| track.try_into().unwrap())
    }
}

impl_publication_trait!(LocalTrackPublication);
impl_publication_trait!(RemoteTrackPublication);
