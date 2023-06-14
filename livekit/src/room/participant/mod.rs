use crate::track::TrackError;
use crate::{prelude::*, DataPacketKind};
use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use livekit_protocol::observer::Dispatcher;
use parking_lot::{Mutex, RwLock, RwLockReadGuard};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

mod local_participant;
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
        track: RemoteTrack,
        publication: RemoteTrackPublication,
    },
    TrackUnsubscribed {
        track: RemoteTrack,
        publication: RemoteTrackPublication,
    },
    TrackSubscriptionFailed {
        error: TrackError,
        sid: TrackSid,
    },
    DataReceived {
        payload: Arc<Vec<u8>>,
        kind: DataPacketKind,
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
    LocalTrackPublished {
        publication: LocalTrackPublication,
    },
    LocalTrackUnpublished {
        publication: LocalTrackPublication,
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

#[derive(Debug, Clone)]
pub enum Participant {
    Local(LocalParticipant),
    Remote(RemoteParticipant),
}

impl Participant {
    enum_dispatch!(
        [Local, Remote];
        pub fn sid(self: &Self) -> ParticipantSid;
        pub fn identity(self: &Self) -> ParticipantIdentity;
        pub fn name(self: &Self) -> String;
        pub fn metadata(self: &Self) -> String;
        pub fn is_speaking(self: &Self) -> bool;
        pub fn audio_level(self: &Self) -> f32;
        pub fn connection_quality(self: &Self) -> ConnectionQuality;
        pub fn tracks(self: &Self) -> RwLockReadGuard<HashMap<TrackSid, TrackPublication>>;
        pub fn register_observer(self: &Self) -> mpsc::UnboundedReceiver<ParticipantEvent>;

        // Internal functions
        pub(crate) fn set_speaking(self: &Self, speaking: bool) -> ();
        pub(crate) fn set_audio_level(self: &Self, level: f32) -> ();
        pub(crate) fn set_connection_quality(self: &Self, quality: ConnectionQuality) -> ();
        pub(crate) fn update_info(self: &Self, info: proto::ParticipantInfo) -> ();
    );
}

#[derive(Debug)]
pub(crate) struct ParticipantInner {
    sid: Mutex<ParticipantSid>,
    identity: Mutex<ParticipantIdentity>,
    name: Mutex<String>,
    metadata: Mutex<String>,
    speaking: AtomicBool,
    tracks: RwLock<HashMap<TrackSid, TrackPublication>>,
    audio_level: AtomicU32,
    connection_quality: AtomicU8,
    dispatcher: Dispatcher<ParticipantEvent>,
}

impl ParticipantInner {
    pub fn new(
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

    pub fn sid(&self) -> ParticipantSid {
        self.sid.lock().clone()
    }

    pub fn identity(&self) -> ParticipantIdentity {
        self.identity.lock().clone()
    }

    pub fn name(&self) -> String {
        self.name.lock().clone()
    }

    pub fn metadata(&self) -> String {
        self.metadata.lock().clone()
    }

    pub fn is_speaking(&self) -> bool {
        self.speaking.load(Ordering::SeqCst)
    }

    pub fn tracks(&self) -> RwLockReadGuard<HashMap<TrackSid, TrackPublication>> {
        self.tracks.read()
    }

    pub fn audio_level(&self) -> f32 {
        f32::from_bits(self.audio_level.load(Ordering::SeqCst))
    }

    pub fn connection_quality(&self) -> ConnectionQuality {
        self.connection_quality.load(Ordering::SeqCst).into()
    }

    pub fn register_observer(&self) -> mpsc::UnboundedReceiver<ParticipantEvent> {
        self.dispatcher.register()
    }

    pub fn update_info(&self, info: proto::ParticipantInfo) {
        *self.sid.lock() = info.sid.into();
        *self.identity.lock() = info.identity.into();
        *self.name.lock() = info.name;
        *self.metadata.lock() = info.metadata; // TODO(theomonnom): callback MetadataChanged
    }

    pub fn set_speaking(&self, speaking: bool) {
        self.speaking.store(speaking, Ordering::SeqCst);
    }

    pub fn set_audio_level(&self, audio_level: f32) {
        self.audio_level
            .store(audio_level.to_bits(), Ordering::SeqCst)
    }

    pub fn set_connection_quality(&self, quality: ConnectionQuality) {
        self.connection_quality
            .store(quality as u8, Ordering::SeqCst);
    }

    pub fn add_track_publication(&self, publication: TrackPublication) {
        self.tracks.write().insert(publication.sid(), publication);
    }
}
