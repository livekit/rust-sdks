use super::TrackPublicationInner;
use crate::id::TrackSid;
use crate::room::TrackEvent;
use crate::track::{
    PermissionStatus, RemoteTrack, SubscriptionStatus, Track, TrackDimension, TrackKind,
    TrackSource,
};
use livekit_protocol as proto;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug)]
struct RemoteInner {
    publication_inner: TrackPublicationInner,
    subscribed: AtomicBool,
    allowed: AtomicBool,
}

#[derive(Clone, Debug)]
pub struct RemoteTrackPublication {
    inner: Arc<RemoteInner>,
}

impl RemoteTrackPublication {
    pub(crate) fn new(info: proto::TrackInfo, track: Option<RemoteTrack>) -> Self {
        Self {
            inner: Arc::new(TrackPublicationInner::new(info, track.map(Into::into))),
        }
    }

    pub fn set_subscribed(&self, subscribed: bool) {
        let old_subscription_state = self.subscription_status();
        let old_permission_state = self.permission_status();
        self.inner.subscribed.store(subscribed, Ordering::Release);

        if subscribed {
            self.inner.allowed.store(true, Ordering::Release);
        }

        self.inner
            .publication_inner
            .dispatcher
            .dispatch(&TrackEvent::SubscriptionStatusChanged {
                old_state: old_subscription_state,
                new_state: self.subscription_status(),
            })
    }

    #[inline]
    pub fn subscription_status(&self) -> SubscriptionStatus {
        if !self.inner.subscribed.load(Ordering::Acquire) {
            return SubscriptionStatus::Unsubscribed;
        }

        if self.inner.publication_inner.track.lock().is_none() {
            return SubscriptionStatus::Desired;
        }

        SubscriptionStatus::Subscribed
    }

    #[inline]
    pub fn permission_status(&self) -> PermissionStatus {
        if self.inner.allowed.load(Ordering::Acquire) {
            PermissionStatus::Allowed
        } else {
            PermissionStatus::NotAllowed
        }
    }

    pub fn is_subscribed(&self) -> bool {
        self.inner.allowed.load(Ordering::Acquire) && self.track().is_some()
    }

    #[inline]
    pub fn sid(&self) -> TrackSid {
        self.inner.publication_inner.sid()
    }

    #[inline]
    pub fn name(&self) -> String {
        self.inner.publication_inner.name()
    }

    #[inline]
    pub fn kind(&self) -> TrackKind {
        self.inner.publication_inner.kind()
    }

    #[inline]
    pub fn source(&self) -> TrackSource {
        self.inner.publication_inner.source()
    }

    #[inline]
    pub fn simulcasted(&self) -> bool {
        self.inner.publication_inner.simulcasted()
    }

    #[inline]
    pub fn dimension(&self) -> TrackDimension {
        self.inner.publication_inner.dimension()
    }

    #[inline]
    pub fn track(&self) -> Option<RemoteTrack> {
        self.inner
            .publication_inner
            .track()
            .map(|track| track.try_into().unwrap())
    }

    #[inline]
    pub fn mime_type(&self) -> String {
        self.inner.publication_inner.mime_type()
    }

    #[inline]
    pub fn is_muted(&self) -> bool {
        self.inner.publication_inner.is_muted()
    }

    #[inline]
    pub fn is_remote(&self) -> bool {
        true
    }

    #[inline]
    pub(crate) fn update_track(&self, track: Option<Track>) {
        self.inner.publication_inner.update_track(track);
    }

    #[inline]
    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        self.inner.publication_inner.update_info(info);
    }
}
