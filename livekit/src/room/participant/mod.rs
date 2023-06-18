use crate::prelude::*;
use crate::rtc_engine::RtcEngine;
use crate::track::TrackError;
use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use livekit_protocol::observer::Dispatcher;
use parking_lot::{RwLock, RwLockReadGuard};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::thread::JoinHandle;
use tokio::sync::{mpsc, oneshot};

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

        pub(crate) fn set_speaking(self: &Self, speaking: bool) -> ();
        pub(crate) fn set_audio_level(self: &Self, level: f32) -> ();
        pub(crate) fn set_connection_quality(self: &Self, quality: ConnectionQuality) -> ();
        pub(crate) fn update_info(self: &Self, info: proto::ParticipantInfo) -> ();
    );
}

#[derive(Debug)]
pub(crate) struct ParticipantInfo {
    pub sid: ParticipantSid,
    pub identity: ParticipantIdentity,
    pub name: String,
    pub metadata: String,
    pub speaking: bool,
    pub audio_level: f32,
    pub connection_quality: ConnectionQuality,
}

#[derive(Debug)]
pub(crate) struct ParticipantInternal {
    pub(super) rtc_engine: Arc<RtcEngine>,
    pub(super) dispatcher: Dispatcher<ParticipantEvent>,
    info: RwLock<ParticipantInfo>,
    tracks: RwLock<HashMap<TrackSid, TrackPublication>>,
    tracks_tasks: RwLock<HashMap<TrackSid, (JoinHandle<()>, oneshot::Sender<()>)>>,
}

impl ParticipantInternal {
    pub fn new(
        rtc_engine: Arc<RtcEngine>,
        sid: ParticipantSid,
        identity: ParticipantIdentity,
        name: String,
        metadata: String,
    ) -> Self {
        Self {
            rtc_engine,
            info: RwLock::new(ParticipantInfo {
                sid,
                identity,
                name,
                metadata,
                speaking: false,
                audio_level: 0.0,
                connection_quality: ConnectionQuality::Unknown,
            }),
            dispatcher: Default::default(),
            tracks: Default::default(),
            tracks_tasks: Default::default(),
        }
    }

    pub fn update_info(&self, new_info: proto::ParticipantInfo) {
        let mut info = self.info.write();
        info.sid = new_info.sid.into();
        info.name = new_info.name;
        info.identity = new_info.identity.into();
        info.metadata = new_info.metadata; // TODO(theomonnom): callback MetadataChanged
    }

    pub fn sid(&self) -> ParticipantSid {
        self.info.read().sid.clone()
    }

    pub fn identity(&self) -> ParticipantIdentity {
        self.info.read().identity.clone()
    }

    pub fn name(&self) -> String {
        self.info.read().name.clone()
    }

    pub fn metadata(&self) -> String {
        self.info.read().metadata.clone()
    }

    pub fn is_speaking(&self) -> bool {
        self.info.read().speaking
    }

    pub fn tracks(&self) -> RwLockReadGuard<HashMap<TrackSid, TrackPublication>> {
        self.tracks.read()
    }

    pub fn audio_level(&self) -> f32 {
        self.info.read().audio_level
    }

    pub fn connection_quality(&self) -> ConnectionQuality {
        self.info.read().connection_quality
    }

    pub fn register_observer(&self) -> mpsc::UnboundedReceiver<ParticipantEvent> {
        self.dispatcher.register()
    }

    pub fn set_speaking(&self, speaking: bool) {
        self.info.write().speaking = speaking;
    }

    pub fn set_audio_level(&self, audio_level: f32) {
        self.info.write().audio_level = audio_level;
    }

    pub fn set_connection_quality(&self, quality: ConnectionQuality) {
        self.info.write().connection_quality = quality;
    }

    pub fn remove_publication(&self, sid: &TrackSid) {
        self.tracks.write().remove(sid);
    }

    pub fn add_publication(&self, publication: TrackPublication) {
        self.tracks.write().insert(publication.sid(), publication);
    }
}
