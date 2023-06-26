use super::track::TrackDimension;
use crate::participant::ParticipantInternal;
use crate::prelude::*;
use crate::track::Track;
use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use parking_lot::RwLock;
use proto::observer::Dispatcher;
use std::sync::Arc;
use std::sync::Weak;
use tokio::sync::Notify;

mod local;
mod remote;

pub use local::*;
pub use remote::*;

#[derive(Debug, Clone)]
pub enum PublicationEvent {
    Muted,
    Unmuted,
    Subscribed,
    Unsubscribed,
    SubscriptionStatusChanged {
        old_state: SubscriptionStatus,
        new_state: SubscriptionStatus,
    },
    SubscriptionPermissionChanged {
        old_state: PermissionStatus,
        new_state: PermissionStatus,
    },
    SubscriptionFailed,
}

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
    );

    pub fn track(&self) -> Option<Track> {
        match self {
            TrackPublication::Local(p) => Some(p.track().into()),
            TrackPublication::Remote(p) => p.track().map(Into::into),
        }
    }
}

#[derive(Debug)]
pub(crate) struct PublicationInfo {
    track: Option<Track>,
    name: String,
    sid: TrackSid,
    kind: TrackKind,
    source: TrackSource,
    simulcasted: bool,
    dimension: TrackDimension,
    mime_type: String,
    muted: bool,
}

#[derive(Debug)]
pub(crate) struct TrackPublicationInner {
    info: RwLock<PublicationInfo>,
    // dispatcher: Dispatcher<PublicationEvent>,
    participant: Weak<ParticipantInternal>,
    //forward_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    // forward_close: Arc<Notify>,
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
            //dispatcher: Default::default(),
            participant,
            //forward_handle: Default::default(),
            //forward_close: Default::default(),
        }
    }

    // Forward track events to the publication events
    // e.g: this also allow us to access the signal_client and notify the server if
    //  a local track changed mute state
    /*async fn track_forward_task(
        close_notifier: Weak<Notify>,
        track: Track,
        dispatcher: Dispatcher<PublicationEvent>,
    ) {
        let mut track_events = track.register_observer();
        loop {
            let notifier = close_notifier.upgrade();
            if notifier.is_none() {
                break;
            }
            let notified = notifier.as_ref().unwrap().notified();

            tokio::select! {
                _ = notified => {
                    break;
                }
                Some(event) = track_events.recv() => {
                    match event {
                        TrackEvent::Muted => {
                            dispatcher.dispatch(&PublicationEvent::Muted);
                        }
                        TrackEvent::Unmuted => {
                            dispatcher.dispatch(&PublicationEvent::Unmuted);
                        }
                    }
                }
            }
        }
    }*/

    pub fn update_track(&self, track: Option<Track>) {
        //let forward_task = self.forward_handle.lock().take();
        //if let Some(task) = forward_task {
        // Make sure to close the old forwarder before changing the track
        //self.forward_close.notify_waiters();
        //let _ = task.await;
        // }

        let mut info = self.info.write();
        info.track = track.clone();

        if let Some(track) = track {
            /*let _handle = tokio::spawn(Self::track_forward_task(
                Arc::downgrade(&self.forward_close),
                track,
                self.dispatcher.clone(),
            ));*/
            //let mut forward_handle = self.forward_handle.lock();
            //*forward_handle = Some(handle);
        }
    }

    // Called when updating a participant info
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

        // TODO MUTE ?????????????????
        // info.muted = new_info.muted;
        // if let Some(track) = info.track.as_ref() {
        //    track.set_muted(info.muted);
        // }
    }

    pub fn participant(&self) -> Weak<ParticipantInternal> {
        self.participant.clone()
    }

    pub fn sid(&self) -> TrackSid {
        self.info.read().sid.clone()
    }

    pub fn name(&self) -> String {
        self.info.read().name.clone()
    }

    pub fn kind(&self) -> TrackKind {
        self.info.read().kind
    }

    pub fn source(&self) -> TrackSource {
        self.info.read().source
    }

    pub fn simulcasted(&self) -> bool {
        self.info.read().simulcasted
    }

    pub fn dimension(&self) -> TrackDimension {
        self.info.read().dimension.clone()
    }

    pub fn mime_type(&self) -> String {
        self.info.read().mime_type.clone()
    }

    pub fn track(&self) -> Option<Track> {
        self.info.read().track.clone()
    }

    pub fn is_muted(&self) -> bool {
        self.info.read().muted
    }
}
