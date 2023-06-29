use super::{PermissionStatus, SubscriptionStatus, TrackPublication, TrackPublicationInner};
use crate::prelude::*;
use livekit_protocol as proto;
use parking_lot::{Mutex, RwLock};
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Default)]
struct RemoteEvents {
    subscribed: Mutex<Option<Box<dyn Fn(RemoteTrackPublication, RemoteTrack) + Send>>>,
    unsubscribed: Mutex<Option<Box<dyn Fn(RemoteTrackPublication, RemoteTrack) + Send>>>,
    subscription_status_changed: Mutex<
        Option<Box<dyn Fn(RemoteTrackPublication, SubscriptionStatus, SubscriptionStatus) + Send>>,
    >, // Old status, new status
    permission_status_changed: Mutex<
        Option<Box<dyn Fn(RemoteTrackPublication, PermissionStatus, PermissionStatus) + Send>>,
    >, // Old status, new status
    subscription_update_needed: Mutex<Option<Box<dyn Fn(RemoteTrackPublication) + Send>>>,
}

#[derive(Debug)]
struct RemoteInfo {
    subscribed: bool,
    allowed: bool,
}

struct RemoteInner {
    info: RwLock<RemoteInfo>,
    events: RemoteEvents,
}

#[derive(Clone)]
pub struct RemoteTrackPublication {
    inner: Arc<TrackPublicationInner>,
    remote: Arc<RemoteInner>,
}

impl Debug for RemoteTrackPublication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteTrackPublication")
            .field("is_subscribed", &self.is_subscribed())
            .field("is_allowed", &self.is_allowed())
            .finish()
    }
}

impl RemoteTrackPublication {
    pub(crate) fn new(info: proto::TrackInfo, track: Option<RemoteTrack>) -> Self {
        Self {
            inner: super::new_inner(info, track.map(Into::into)),
            remote: Arc::new(RemoteInner {
                info: RwLock::new(RemoteInfo {
                    subscribed: false,
                    allowed: false,
                }),
                events: Default::default(),
            }),
        }
    }

    /// This is called by the RemoteParticipant when it successfully subscribe to the track or when
    /// the track is being unsubscribed.
    /// We register the mute events from the track here so we can forward them.
    pub(crate) fn set_track(&self, track: Option<RemoteTrack>) {
        let old_subscription_state = self.subscription_status();
        let old_permission_state = self.permission_status();

        let prev_track = self.track();

        if let Some(prev_track) = prev_track {
            if let Some(unsubscribed) = self.remote.events.unsubscribed.lock().as_ref() {
                unsubscribed(self.clone(), prev_track);
            }
        }

        super::set_track(
            &self.inner,
            &TrackPublication::Remote(self.clone()),
            track.clone().map(Into::into),
        );

        if let Some(track) = track {
            if let Some(subscribed) = self.remote.events.subscribed.lock().as_ref() {
                subscribed(self.clone(), track);
            }
        }

        self.emit_subscription_update(old_subscription_state);
        self.emit_permission_update(old_permission_state);
    }

    pub(crate) fn emit_subscription_update(&self, old_subscription_state: SubscriptionStatus) {
        if old_subscription_state != self.subscription_status() {
            if let Some(subscription_status_changed) = self
                .remote
                .events
                .subscription_status_changed
                .lock()
                .as_ref()
            {
                subscription_status_changed(
                    self.clone(),
                    old_subscription_state,
                    self.subscription_status(),
                );
            }
        }
    }

    pub(crate) fn emit_permission_update(&self, old_permission_state: PermissionStatus) {
        if old_permission_state != self.permission_status() {
            if let Some(subscription_permission_changed) =
                self.remote.events.permission_status_changed.lock().as_ref()
            {
                subscription_permission_changed(
                    self.clone(),
                    old_permission_state,
                    self.permission_status(),
                );
            }
        }
    }

    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        super::update_info(&self.inner, &TrackPublication::Remote(self.clone()), info);
    }

    pub(crate) fn on_muted(&self, f: impl Fn(TrackPublication, Track) + Send + 'static) {
        *self.inner.events.muted.lock() = Some(Box::new(f));
    }

    pub(crate) fn on_unmuted(&self, f: impl Fn(TrackPublication, Track) + Send + 'static) {
        *self.inner.events.unmuted.lock() = Some(Box::new(f));
    }

    pub(crate) fn on_subscribed(
        &self,
        f: impl Fn(RemoteTrackPublication, RemoteTrack) + Send + 'static,
    ) {
        *self.remote.events.subscribed.lock() = Some(Box::new(f));
    }

    pub(crate) fn on_unsubscribed(
        &self,
        f: impl Fn(RemoteTrackPublication, RemoteTrack) + Send + 'static,
    ) {
        *self.remote.events.unsubscribed.lock() = Some(Box::new(f));
    }

    pub(crate) fn on_subscription_status_changed(
        &self,
        f: impl Fn(RemoteTrackPublication, SubscriptionStatus, SubscriptionStatus) + Send + 'static,
    ) {
        *self.remote.events.subscription_status_changed.lock() = Some(Box::new(f));
    }

    pub(crate) fn on_permission_status_changed(
        &self,
        f: impl Fn(RemoteTrackPublication, PermissionStatus, PermissionStatus) + Send + 'static,
    ) {
        *self.remote.events.permission_status_changed.lock() = Some(Box::new(f));
    }

    pub(crate) fn on_subscription_update_needed(
        &self,
        f: impl Fn(RemoteTrackPublication) + Send + 'static,
    ) {
        *self.remote.events.subscription_update_needed.lock() = Some(Box::new(f));
    }

    pub async fn set_subscribed(&self, subscribed: bool) {
        let old_subscription_state = self.subscription_status();
        let old_permission_state = self.permission_status();

        let mut info = self.remote.info.write();
        info.subscribed = subscribed;

        if subscribed {
            info.allowed = true;
        }

        // Request to send an update to the SFU
        if let Some(subscription_update_needed) = self
            .remote
            .events
            .subscription_update_needed
            .lock()
            .as_ref()
        {
            subscription_update_needed(self.clone());
        }

        self.emit_subscription_update(old_subscription_state);
        self.emit_permission_update(old_permission_state);
    }

    pub fn subscription_status(&self) -> SubscriptionStatus {
        if !self.is_subscribed() {
            return SubscriptionStatus::Unsubscribed;
        }

        if self.track().is_none() {
            return SubscriptionStatus::Desired;
        }

        SubscriptionStatus::Subscribed
    }

    pub fn permission_status(&self) -> PermissionStatus {
        if self.is_allowed() {
            PermissionStatus::Allowed
        } else {
            PermissionStatus::NotAllowed
        }
    }

    pub fn is_subscribed(&self) -> bool {
        self.is_allowed() && self.track().is_some()
    }

    pub fn is_allowed(&self) -> bool {
        self.remote.info.read().allowed
    }

    pub fn sid(&self) -> TrackSid {
        self.inner.info.read().sid.clone()
    }

    pub fn name(&self) -> String {
        self.inner.info.read().name.clone()
    }

    pub fn kind(&self) -> TrackKind {
        self.inner.info.read().kind
    }

    pub fn source(&self) -> TrackSource {
        self.inner.info.read().source
    }

    pub fn simulcasted(&self) -> bool {
        self.inner.info.read().simulcasted
    }

    pub fn dimension(&self) -> TrackDimension {
        self.inner.info.read().dimension.clone()
    }

    pub fn track(&self) -> Option<RemoteTrack> {
        self.inner
            .info
            .read()
            .track
            .clone()
            .map(|track| track.try_into().unwrap())
    }

    pub fn mime_type(&self) -> String {
        self.inner.info.read().mime_type.clone()
    }

    pub fn is_muted(&self) -> bool {
        self.inner.info.read().muted
    }

    pub fn is_remote(&self) -> bool {
        true
    }
}
