use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8};
use parking_lot::Mutex;
use crate::room::id::TrackSid;

pub(super) struct TrackPublicationShared {
    pub(super) name: Mutex<String>,
    pub(super) sid: Mutex<TrackSid>,
    pub(super) kind: AtomicU8, // Casted to TrackKind
    pub(super) source: AtomicU8, // Casted to TrackSource
    pub(super) simulcasted: AtomicBool
}

#[derive(Clone)]
pub struct LocalTrackPublication {
    shared: Arc<TrackPublicationShared>
}

#[derive(Clone)]
pub struct RemoteTrackPublication {
    shared: Arc<TrackPublicationShared>
}

#[derive(Clone)]
pub enum TrackPublication {
    Local(LocalTrackPublication),
    Remote(RemoteTrackPublication)
}