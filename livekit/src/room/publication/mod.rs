use super::track::TrackDimension;
use crate::participant::ParticipantInternal;
use crate::prelude::*;
use crate::track::Track;
use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use parking_lot::{Mutex, RwLock};
use std::sync::{Arc, Weak};

mod local;
mod remote;

pub use local::*;
pub use remote::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionStatus {
    Desired,
    Subscribed,
    Unsubscribed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStatus {
    Allowed,
    NotAllowed,
}

#[derive(Clone, Debug)]
pub enum TrackPublication {
    Local(LocalTrackPublication),
    Remote(RemoteTrackPublication),
}

impl TrackPublication {
    enum_dispatch!(
        [Local, Remote];
        pub fn sid(self: &Self) -> TrackSid;
        pub fn name(self: &Self) -> String;
        pub fn kind(self: &Self) -> TrackKind;
        pub fn source(self: &Self) -> TrackSource;
        pub fn simulcasted(self: &Self) -> bool;
        pub fn dimension(self: &Self) -> TrackDimension;
        pub fn mime_type(self: &Self) -> String;
        pub fn is_muted(self: &Self) -> bool;
        pub fn is_remote(self: &Self) -> bool;

        pub(crate) fn on_muted(self: &Self, on_mute: impl Fn() + Send) -> ();
        pub(crate) fn on_unmuted(self: &Self, on_unmute: impl Fn() + Send) -> ();
        pub(crate) fn update_info(self: &Self, info: proto::TrackInfo) -> ();
    );

    // pub(crate) fn set_track(self: &Self, track: Option<Track>) -> ();

    pub(crate) fn set_track(&self, track: Option<Track>) {
        match self {
            TrackPublication::Local(p) => p.set_track(track),
            TrackPublication::Remote(p) => p.set_track(track.map(|t| t.try_into().unwrap())),
        }
    }

    pub fn track(&self) -> Option<Track> {
        match self {
            TrackPublication::Local(p) => Some(p.track().into()),
            TrackPublication::Remote(p) => p.track().map(Into::into),
        }
    }
}

struct PublicationInfo {
    pub track: Option<Track>,
    pub name: String,
    pub sid: TrackSid,
    pub kind: TrackKind,
    pub source: TrackSource,
    pub simulcasted: bool,
    pub dimension: TrackDimension,
    pub mime_type: String,
    pub muted: bool,
}

#[derive(Default)]
struct PublicationEvents {
    pub muted: Mutex<Option<Box<dyn Fn() + Send>>>,
    pub unmuted: Mutex<Option<Box<dyn Fn() + Send>>>,
}

pub(super) struct TrackPublicationInner {
    pub info: RwLock<PublicationInfo>,
    pub participant: Weak<ParticipantInternal>,
    pub events: Arc<PublicationEvents>,
}

impl TrackPublicationInner {
    pub fn new(
        info: proto::TrackInfo,
        participant: Weak<ParticipantInternal>,
        track: Option<Track>,
    ) -> Self {
        let info = PublicationInfo {
            track,
            name: info.name,
            sid: info.sid.into(),
            kind: proto::TrackType::from_i32(info.r#type)
                .unwrap()
                .try_into()
                .unwrap(),
            source: proto::TrackSource::from_i32(info.source)
                .unwrap()
                .try_into()
                .unwrap(),
            simulcasted: info.simulcast,
            dimension: TrackDimension(info.width, info.height),
            mime_type: info.mime_type,
            muted: info.muted,
        };

        Self {
            info: RwLock::new(info),
            participant,
            events: Default::default(),
        }
    }

    pub fn update_info(&self, new_info: proto::TrackInfo) {
        let mut info = self.info.write();
        info.name = new_info.name;
        info.sid = new_info.sid.into();
        info.dimension = TrackDimension(new_info.width, new_info.height);
        info.mime_type = new_info.mime_type;
        info.kind =
            TrackKind::try_from(proto::TrackType::from_i32(new_info.r#type).unwrap()).unwrap();
        info.source = TrackSource::from(proto::TrackSource::from_i32(new_info.source).unwrap());
        info.simulcasted = new_info.simulcast;
    }

    pub fn set_track(&self, track: Option<Track>) {
        let mut info = self.info.write();
        if let Some(prev_track) = info.track.as_ref() {
            prev_track.on_muted(|| {});
            prev_track.on_unmuted(|| {});
        }

        info.track = track.clone();

        if let Some(track) = track.as_ref() {
            info.sid = track.sid();

            let events = self.events.clone();
            track.on_muted(move || {
                if let Some(on_muted) = events.muted.lock().as_ref() {
                    on_muted();
                }
            });

            let events = self.events.clone();
            track.on_unmuted(move || {
                if let Some(on_unmuted) = events.unmuted.lock().as_ref() {
                    on_unmuted();
                }
            });
        }
    }
}
