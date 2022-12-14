use crate::proto::TrackType;
use crate::proto::{TrackInfo, TrackSource as ProtoTrackSource};
use crate::room::id::ParticipantSid;
use crate::room::id::TrackSid;
use crate::room::track::local_track::LocalTrackHandle;
use crate::room::track::remote_track::RemoteTrackHandle;
use crate::room::track::{TrackHandle, TrackKind, TrackSource};
use livekit_utils::enum_dispatch;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use super::track::TrackDimension;

pub(crate) trait TrackPublicationInternalTrait {
    fn update_track(&self, track: Option<TrackHandle>);
    fn update_info(&self, info: TrackInfo);
}

pub trait TrackPublicationTrait {
    fn name(&self) -> String;
    fn sid(&self) -> TrackSid;
    fn kind(&self) -> TrackKind;
    fn source(&self) -> TrackSource;
    fn simulcasted(&self) -> bool;
}

#[derive(Debug)]
pub(super) struct TrackPublicationShared {
    pub(super) track: Mutex<Option<TrackHandle>>,
    pub(super) name: Mutex<String>,
    pub(super) sid: Mutex<TrackSid>,
    pub(super) kind: AtomicU8,   // Casted to TrackKind
    pub(super) source: AtomicU8, // Casted to TrackSource
    pub(super) simulcasted: AtomicBool,
    pub(super) dimension: Mutex<TrackDimension>,
    pub(super) mime_type: Mutex<String>,
    pub(super) participant: ParticipantSid, // TODO(theomonnom) Use WeakParticipant instead
}

impl TrackPublicationShared {
    pub fn new(
        info: TrackInfo,
        participant: ParticipantSid,
        track: Option<TrackHandle>,
    ) -> Arc<Self> {
        Arc::new(Self {
            track: Mutex::new(track),
            name: Mutex::new(info.name),
            sid: Mutex::new(info.sid.into()),
            kind: AtomicU8::new(TrackKind::from(TrackType::from_i32(info.r#type).unwrap()) as u8),
            source: AtomicU8::new(TrackSource::from(
                ProtoTrackSource::from_i32(info.source).unwrap(),
            ) as u8),
            simulcasted: AtomicBool::new(info.simulcast),
            dimension: Mutex::new(TrackDimension(info.width, info.height)),
            mime_type: Mutex::new(info.mime_type),
            participant,
        })
    }

    pub fn update_info(&self, info: TrackInfo) {
        *self.name.lock() = info.name;
        *self.sid.lock() = info.sid.into();
        self.kind.store(
            TrackKind::from(TrackType::from_i32(info.r#type).unwrap()) as u8,
            Ordering::SeqCst,
        );
        self.source.store(
            TrackSource::from(ProtoTrackSource::from_i32(info.source).unwrap()) as u8,
            Ordering::SeqCst,
        );
        self.simulcasted.store(info.simulcast, Ordering::SeqCst);
        *self.dimension.lock() = TrackDimension(info.width, info.height);
        *self.mime_type.lock() = info.mime_type;
    }
}

#[derive(Clone, Debug)]
pub enum TrackPublication {
    Local(LocalTrackPublication),
    Remote(RemoteTrackPublication),
}

impl TrackPublication {
    pub fn track(&self) -> Option<TrackHandle> {
        // Not calling Local/Remote function here, we don't need "cast"
        match self {
            TrackPublication::Local(p) => p.shared.track.lock().clone(),
            TrackPublication::Remote(p) => p.shared.track.lock().clone(),
        }
    }
}

impl TrackPublicationInternalTrait for TrackPublication {
    enum_dispatch!(
        [Local, Remote]
        fnc!(update_track, &Self, [track: Option<TrackHandle>], ());
        fnc!(update_info, &Self, [info: TrackInfo], ());
    );
}

impl TrackPublicationTrait for TrackPublication {
    enum_dispatch!(
        [Local, Remote]
        fnc!(sid, &Self, [], TrackSid);
        fnc!(name, &Self, [], String);
        fnc!(kind, &Self, [], TrackKind);
        fnc!(source, &Self, [], TrackSource);
        fnc!(simulcasted, &Self, [], bool);
    );
}

macro_rules! impl_publication_trait {
    ($x:ident) => {
        impl TrackPublicationInternalTrait for $x {
            fn update_track(&self, track: Option<TrackHandle>) {
                *self.shared.track.lock() = track;
            }

            fn update_info(&self, info: TrackInfo) {
                self.shared.update_info(info);
            }
        }

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

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct RemoteTrackPublication {
    shared: Arc<TrackPublicationShared>,
}

impl RemoteTrackPublication {
    pub fn new(info: TrackInfo, participant: ParticipantSid, track: Option<TrackHandle>) -> Self {
        Self {
            shared: TrackPublicationShared::new(info, participant, track),
        }
    }

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
