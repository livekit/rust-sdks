use crate::prelude::*;
use crate::proto;
use crate::track::TrackError;
use livekit_utils::enum_dispatch;
use livekit_utils::observer::Dispatcher;
use parking_lot::{Mutex, RwLock, RwLockReadGuard};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

mod local_participant;
mod publish_utils;
mod remote_participant;

pub use local_participant::*;
pub use remote_participant::*;

#[derive(Debug, Clone)]
pub enum ParticipantEvent {
    TrackPublished {
        publication: RemoteTrackPublication,
    },
    TrackUnpublished {
        publication: RemoteTrackPublication,
    },
    TrackSubscribed {
        track: RemoteTrackHandle,
        publication: RemoteTrackPublication,
    },
    TrackUnsubscribed {
        track: RemoteTrackHandle,
        publication: RemoteTrackPublication,
    },
    TrackSubscriptionFailed {
        error: TrackError,
        sid: TrackSid,
    },
    DataReceived {
        payload: Arc<Vec<u8>>,
        kind: proto::data_packet::Kind,
    },
    SpeakingChanged {
        speaking: bool,
    },
    TrackMuted {
        publication: TrackPublication,
    },
    TrackUnmuted {
        publication: TrackPublication,
    },
    ConnectionQualityChanged {
        quality: ConnectionQuality,
    },
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
pub enum ConnectionQuality {
    Unknown,
    Excellent,
    Good,
    Poor,
}

impl From<u8> for ConnectionQuality {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Excellent,
            2 => Self::Good,
            3 => Self::Poor,
            _ => Self::Unknown,
        }
    }
}

impl From<proto::ConnectionQuality> for ConnectionQuality {
    fn from(value: proto::ConnectionQuality) -> Self {
        match value {
            proto::ConnectionQuality::Excellent => Self::Excellent,
            proto::ConnectionQuality::Good => Self::Good,
            proto::ConnectionQuality::Poor => Self::Poor,
        }
    }
}

#[derive(Debug)]
pub(super) struct ParticipantShared {
    pub(super) sid: Mutex<ParticipantSid>,
    pub(super) identity: Mutex<ParticipantIdentity>,
    pub(super) name: Mutex<String>,
    pub(super) metadata: Mutex<String>,
    pub(super) tracks: RwLock<HashMap<TrackSid, TrackPublication>>,
    pub(super) speaking: AtomicBool,
    pub(super) audio_level: AtomicU32,
    pub(super) connection_quality: AtomicU8,
    pub(super) dispatcher: Mutex<Dispatcher<ParticipantEvent>>,
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
            speaking: Default::default(),
            audio_level: Default::default(),
            connection_quality: AtomicU8::new(ConnectionQuality::Unknown as u8),
            dispatcher: Default::default(),
        }
    }

    pub(crate) fn update_info(&self, info: proto::ParticipantInfo) {
        *self.sid.lock() = info.sid.into();
        *self.identity.lock() = info.identity.into();
        *self.name.lock() = info.name;
        *self.metadata.lock() = info.metadata; // TODO(theomonnom): callback MetadataChanged
    }

    pub(crate) fn set_speaking(&self, speaking: bool) {
        self.speaking.store(speaking, Ordering::SeqCst);
    }

    pub(crate) fn set_audio_level(&self, audio_level: f32) {
        self.audio_level
            .store(audio_level.to_bits(), Ordering::SeqCst)
    }

    pub(crate) fn register_observer(&self) -> mpsc::UnboundedReceiver<ParticipantEvent> {
        self.dispatcher.lock().register()
    }

    pub(crate) fn set_connection_quality(&self, quality: ConnectionQuality) {
        self.connection_quality
            .store(quality as u8, Ordering::SeqCst);
    }

    pub(crate) fn add_track_publication(&self, publication: TrackPublication) {
        self.tracks.write().insert(publication.sid(), publication);
    }
}

pub(crate) trait ParticipantInternalTrait {
    fn set_speaking(&self, speaking: bool);
    fn set_audio_level(&self, level: f32);
    fn set_connection_quality(&self, quality: ConnectionQuality);
    fn update_info(self: &Arc<Self>, info: proto::ParticipantInfo, emit_events: bool);
}

pub trait ParticipantTrait {
    fn sid(&self) -> ParticipantSid;
    fn identity(&self) -> ParticipantIdentity;
    fn name(&self) -> String;
    fn metadata(&self) -> String;
    fn is_speaking(&self) -> bool;
    fn audio_level(&self) -> f32;
    fn connection_quality(&self) -> ConnectionQuality;
    fn tracks(&self) -> RwLockReadGuard<HashMap<TrackSid, TrackPublication>>;
    fn register_observer(&self) -> mpsc::UnboundedReceiver<ParticipantEvent>;
}

#[derive(Debug, Clone)]
pub enum Participant {
    Local(Arc<LocalParticipant>),
    Remote(Arc<RemoteParticipant>),
}

// TODO(theomonnom): Should I provide a WeakParticipant here ?

impl Participant {
    enum_dispatch!(
        [Local, Remote]
        fnc!(pub(crate), update_info, &Self, [info: proto::ParticipantInfo, emit_events: bool], ());
        fnc!(pub(crate), set_speaking, &Self, [speaking: bool], ());
        fnc!(pub(crate), set_audio_level, &Self, [audio_level: f32], ());
        fnc!(pub(crate), set_connection_quality, &Self, [quality: ConnectionQuality], ());
    );
}

impl ParticipantTrait for Participant {
    enum_dispatch!(
        [Local, Remote]
        fnc!(sid, &Self, [], ParticipantSid);
        fnc!(identity, &Self, [], ParticipantIdentity);
        fnc!(name, &Self, [], String);
        fnc!(metadata, &Self, [], String);
        fnc!(is_speaking, &Self, [], bool);
        fnc!(audio_level, &Self, [], f32);
        fnc!(connection_quality, &Self, [], ConnectionQuality);
        fnc!(tracks, &Self, [], RwLockReadGuard<HashMap<TrackSid, TrackPublication>>);
        fnc!(register_observer, &Self, [], mpsc::UnboundedReceiver<ParticipantEvent>);
    );
}

macro_rules! impl_participant_trait {
    ($x:ty) => {
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

            fn is_speaking(&self) -> bool {
                self.shared.speaking.load(Ordering::SeqCst)
            }

            fn audio_level(&self) -> f32 {
                f32::from_bits(self.shared.audio_level.load(Ordering::SeqCst))
            }

            fn connection_quality(&self) -> ConnectionQuality {
                self.shared.connection_quality.load(Ordering::SeqCst).into()
            }

            fn tracks(&self) -> RwLockReadGuard<HashMap<TrackSid, TrackPublication>> {
                self.shared.tracks.read()
            }

            fn register_observer(&self) -> mpsc::UnboundedReceiver<ParticipantEvent> {
                self.shared.register_observer()
            }
        }
    };
}

pub(super) use impl_participant_trait;
