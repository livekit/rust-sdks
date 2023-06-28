use super::{PermissionStatus, SubscriptionStatus, TrackPublicationInner};
use crate::id::TrackSid;
use crate::participant::ParticipantInternal;
use crate::track::{RemoteTrack, TrackDimension, TrackKind, TrackSource};
use livekit_protocol as proto;
use parking_lot::RwLock;
use std::fmt::Debug;
use std::sync::{Arc, Weak};

#[derive(Default)]
struct RemoteEvents {
    subscribed: Option<Arc<dyn Fn(RemoteTrack)>>,
    unsubscribed: Option<Arc<dyn Fn(RemoteTrack)>>,
    subscription_status_changed: Option<Arc<dyn Fn(SubscriptionStatus, SubscriptionStatus)>>, // Old status, new status
    permission_status_changed: Option<Arc<dyn Fn(PermissionStatus, PermissionStatus)>>, // Old status, new status
    subscription_failed: Option<Arc<dyn Fn()>>,
}

#[derive(Debug)]
struct RemoteInfo {
    subscribed: bool,
    allowed: bool,
}

struct RemoteInner {
    info: RwLock<RemoteInfo>,
    events: RwLock<RemoteEvents>,
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
    pub(crate) fn new(
        info: proto::TrackInfo,
        participant: Weak<ParticipantInternal>,
        track: Option<RemoteTrack>,
    ) -> Self {
        Self {
            inner: Arc::new(TrackPublicationInner::new(
                info,
                participant,
                track.map(Into::into),
            )),
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
        let prev_track = self.track();

        if let Some(prev_track) = prev_track {
            if let Some(unsubscribed) = self.remote.events.read().unsubscribed.clone() {
                unsubscribed(prev_track);
            }
        }

        self.inner.set_track(track.clone().map(Into::into));

        if let Some(track) = track {
            if let Some(subscribed) = self.remote.events.read().subscribed.clone() {
                subscribed(track);
            }
        }
    }

    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        self.inner.update_info(info);
    }

    pub(crate) fn on_muted(&self, f: impl Fn()) {
        self.inner.events.write().muted = Some(Arc::new(f));
    }

    pub(crate) fn on_unmuted(&self, f: impl Fn()) {
        self.inner.events.write().unmuted = Some(Arc::new(f));
    }

    pub async fn set_subscribed(&self, subscribed: bool) {
        let old_subscription_state = self.subscription_status();
        let old_permission_state = self.permission_status();

        let mut info = self.remote.info.write();
        info.subscribed = subscribed;

        if subscribed {
            info.allowed = true;
        }

        let participant = self.inner.participant.upgrade();
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

        let _ = participant
            .rtc_engine
            .send_request(proto::signal_request::Message::Subscription(
                update_subscription,
            ))
            .await;

        if old_subscription_state != self.subscription_status() {
            if let Some(subscription_status_changed) =
                &self.remote.events.read().subscription_status_changed
            {
                subscription_status_changed(old_subscription_state, self.subscription_status());
            }
        }

        if old_permission_state != self.permission_status() {
            if let Some(subscription_permission_changed) =
                &self.remote.events.read().permission_status_changed
            {
                subscription_permission_changed(old_permission_state, self.permission_status());
            }
        }
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
