use futures_util::future::BoxFuture;
use thiserror::Error;

type EventHandler<T> = Box<dyn FnMut(T) -> BoxFuture<'static, ()> + Send + Sync>;

macro_rules! event_setter {
    ($fnc:ident, $event:ty) => {
        pub fn $fnc<F, Fut>(&self, mut callback: F)
        where
            F: FnMut($event) -> Fut + Send + Sync + 'static,
            Fut: Future<Output = ()> + Send + Sync + 'static,
        {
            *self.$fnc.lock() = Some(Box::new(move |event| Box::pin(callback(event))));
        }
    };
}

#[derive(Error, Debug, Clone)]
pub enum TrackError {
    #[error("could not find published track with sid: {0}")]
    TrackNotFound(String),
}

pub mod room {
    use super::{EventHandler, TrackError};
    use crate::room::id::TrackSid;
    use crate::room::participant::remote_participant::RemoteParticipant;
    use crate::room::publication::RemoteTrackPublication;
    use crate::room::track::remote_track::RemoteTrackHandle;
    use crate::room::RoomHandle;
    use futures::future::Future;
    use parking_lot::Mutex;
    use std::sync::Arc;

    #[derive(Clone)]
    pub struct ParticipantConnectedEvent {
        pub room_handle: RoomHandle,
        pub participant: Arc<RemoteParticipant>,
    }

    #[derive(Clone)]
    pub struct ParticipantDisconnectedEvent {
        pub room_handle: RoomHandle,
        pub participant: Arc<RemoteParticipant>,
    }

    #[derive(Clone)]
    pub struct TrackSubscribedEvent {
        pub room_handle: RoomHandle,
        pub track: RemoteTrackHandle,
        pub publication: RemoteTrackPublication,
        pub participant: Arc<RemoteParticipant>,
    }

    #[derive(Clone)]
    pub struct TrackPublishedEvent {
        pub room_handle: RoomHandle,
        pub publication: RemoteTrackPublication,
        pub participant: Arc<RemoteParticipant>,
    }

    #[derive(Clone)]
    pub struct TrackSubscriptionFailedEvent {
        pub room_handle: RoomHandle,
        pub error: TrackError,
        pub sid: TrackSid,
        pub participant: Arc<RemoteParticipant>,
    }

    pub(crate) type OnParticipantConnectedHandler = EventHandler<ParticipantConnectedEvent>;
    pub(crate) type OnParticipantDisconnectedHandler = EventHandler<ParticipantDisconnectedEvent>;
    pub(crate) type OnTrackSubscribedEventHandler = EventHandler<TrackSubscribedEvent>;
    pub(crate) type OnTrackPublishedEventHandler = EventHandler<TrackPublishedEvent>;
    pub(crate) type OnTrackSubscriptionFailedHandler = EventHandler<TrackSubscriptionFailedEvent>;

    #[derive(Default)]
    pub struct RoomEvents {
        pub(crate) on_participant_connected: Mutex<Option<OnParticipantConnectedHandler>>,
        pub(crate) on_participant_disconnected: Mutex<Option<OnParticipantDisconnectedHandler>>,
        pub(crate) on_track_subscribed: Mutex<Option<OnTrackSubscribedEventHandler>>,
        pub(crate) on_track_published: Mutex<Option<OnTrackPublishedEventHandler>>,
        pub(crate) on_track_subscription_failed: Mutex<Option<OnTrackSubscriptionFailedHandler>>,
    }

    impl RoomEvents {
        event_setter!(on_participant_connected, ParticipantConnectedEvent);
        event_setter!(on_participant_disconnected, ParticipantDisconnectedEvent);
        event_setter!(on_track_subscribed, TrackSubscribedEvent);
        event_setter!(on_track_published, TrackPublishedEvent);
        event_setter!(on_track_subscription_failed, TrackSubscriptionFailedEvent);
    }
}

pub mod participant {
    use super::{EventHandler, TrackError};
    use crate::room::id::TrackSid;
    use crate::room::participant::remote_participant::RemoteParticipant;
    use crate::room::publication::RemoteTrackPublication;
    use crate::room::track::remote_track::RemoteTrackHandle;
    use futures::future::Future;
    use parking_lot::Mutex;
    use std::sync::Arc;

    #[derive(Clone)]
    pub struct TrackPublishedEvent {
        pub publication: RemoteTrackPublication,
        pub participant: Arc<RemoteParticipant>,
    }

    #[derive(Clone)]
    pub struct TrackSubscribedEvent {
        pub track: RemoteTrackHandle,
        pub publication: RemoteTrackPublication,
        pub participant: Arc<RemoteParticipant>,
    }

    #[derive(Clone)]
    pub struct TrackSubscriptionFailedEvent {
        pub sid: TrackSid,
        pub error: TrackError,
        pub participant: Arc<RemoteParticipant>,
    }

    pub(crate) type TrackPublishedHandler = EventHandler<TrackPublishedEvent>;
    pub(crate) type TrackSubscribedHandler = EventHandler<TrackSubscribedEvent>;
    pub(crate) type TrackSubscriptionFailedHandler = EventHandler<TrackSubscriptionFailedEvent>;

    #[derive(Default)]
    pub struct ParticipantEvents {
        pub(crate) on_track_published: Mutex<Option<TrackPublishedHandler>>,
        pub(crate) on_track_subscribed: Mutex<Option<TrackSubscribedHandler>>,
        pub(crate) on_track_subscription_failed: Mutex<Option<TrackSubscriptionFailedHandler>>,
    }

    impl ParticipantEvents {
        event_setter!(on_track_published, TrackPublishedEvent);
        event_setter!(on_track_subscribed, TrackSubscribedEvent);
        event_setter!(on_track_subscription_failed, TrackSubscriptionFailedEvent);
    }
}
