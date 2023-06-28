use crate::prelude::*;
use crate::rtc_engine::RtcEngine;
use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use parking_lot::{RwLock, RwLockReadGuard};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

mod local_participant;
mod remote_participant;

pub use local_participant::*;
pub use remote_participant::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
pub enum ConnectionQuality {
    Unknown,
    Excellent,
    Good,
    Poor,
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


        pub(crate) fn update_info(self: &Self, info: proto::ParticipantInfo) -> ();

        // Internal functions called by the Room when receiving the associated signal messages
        pub(crate) fn set_speaking(self: &Self, speaking: bool) -> ();
        pub(crate) fn set_audio_level(self: &Self, level: f32) -> ();
        pub(crate) fn set_connection_quality(self: &Self, quality: ConnectionQuality) -> ();
    );
}

struct ParticipantInfo {
    pub sid: ParticipantSid,
    pub identity: ParticipantIdentity,
    pub name: String,
    pub metadata: String,
    pub speaking: bool,
    pub audio_level: f32,
    pub connection_quality: ConnectionQuality,
}

pub(super) struct ParticipantInternal {
    pub rtc_engine: Arc<RtcEngine>,
    pub info: RwLock<ParticipantInfo>,
    pub tracks: RwLock<HashMap<TrackSid, TrackPublication>>,
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
            tracks: Default::default(),
        }
    }

    pub fn update_info(&self, new_info: proto::ParticipantInfo) {
        let mut info = self.info.write();
        info.sid = new_info.sid.into();
        info.name = new_info.name;
        info.identity = new_info.identity.into();
        info.metadata = new_info.metadata; // TODO(theomonnom): callback MetadataChanged
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
