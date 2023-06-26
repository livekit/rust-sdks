use super::{PermissionStatus, SubscriptionStatus, TrackPublicationInner};
use crate::id::TrackSid;
use crate::participant::ParticipantInternal;
use crate::publication::PublicationEvent;
use crate::track::{RemoteTrack, Track, TrackDimension, TrackKind, TrackSource};
use livekit_protocol as proto;
use parking_lot::RwLock;
use std::sync::{Arc, Weak};

#[derive(Debug)]
struct RemoteInfo {
    subscribed: bool,
    allowed: bool,
}

#[derive(Debug)]
struct RemoteInner {
    publication_inner: TrackPublicationInner,
    info: RwLock<RemoteInfo>,
}

#[derive(Clone, Debug)]
pub struct RemoteTrackPublication {
    inner: Arc<RemoteInner>,
}

impl RemoteTrackPublication {
    pub(crate) fn new(
        info: proto::TrackInfo,
        participant: Weak<ParticipantInternal>,
        track: Option<RemoteTrack>,
    ) -> Self {
        Self {
            inner: Arc::new(RemoteInner {
                publication_inner: TrackPublicationInner::new(
                    info,
                    participant,
                    track.map(Into::into),
                ),
                info: RwLock::new(RemoteInfo {
                    subscribed: false,
                    allowed: false,
                }),
            }),
        }
    }

    pub fn set_subscribed(&self, subscribed: bool) {
        let old_subscription_state = self.subscription_status();
        let old_permission_state = self.permission_status();
        let mut info = self.inner.info.write();
        info.subscribed = subscribed;

        if subscribed {
            info.allowed = true;
        }

        let participant = self.inner.publication_inner.participant.upgrade();
        if participant.is_none() {
            log::warn!("publication's participant is invalid, set_subscribed failed");
            return;
        }
        let participant = participant.unwrap();

        let update_subscription = proto::UpdateSubscription {
            track_sids: vec![self.sid().0],
            subscribe: subscribed,
            participant_tracks: vec![proto::ParticipantTracks {
                participant_sid: participant.sid().0,
                track_sids: vec![self.sid().0],
            }],
        };

        // Engine update subscription

        if old_subscription_state != self.subscription_status() {
            self.inner.publication_inner.dispatcher.dispatch(
                &PublicationEvent::SubscriptionStatusChanged {
                    old_state: old_subscription_state,
                    new_state: self.subscription_status(),
                },
            )
        }

        if old_permission_state != self.permission_status() {
            self.inner.publication_inner.dispatcher.dispatch(
                &PublicationEvent::SubscriptionPermissionChanged {
                    old_state: old_permission_state,
                    new_state: self.permission_status(),
                },
            )
        }
    }

    #[inline]
    pub fn subscription_status(&self) -> SubscriptionStatus {
        if !self.inner.info.read().subscribed {
            return SubscriptionStatus::Unsubscribed;
        }

        if self.track().is_none() {
            return SubscriptionStatus::Desired;
        }

        SubscriptionStatus::Subscribed
    }

    #[inline]
    pub fn permission_status(&self) -> PermissionStatus {
        if self.inner.info.read().allowed {
            PermissionStatus::Allowed
        } else {
            PermissionStatus::NotAllowed
        }
    }

    pub fn is_subscribed(&self) -> bool {
        self.inner.info.read().allowed && self.track().is_some()
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
