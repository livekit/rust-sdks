use crate::room::id::TrackSid;
use crate::room::participant::remote_participant::RemoteParticipant;
use crate::room::publication::RemoteTrackPublication;
use crate::room::room_session::{ConnectionState, RoomSession};
use crate::room::track::remote_track::RemoteTrackHandle;
use futures::future::Future;
use futures_util::future::BoxFuture;
use parking_lot::Mutex;
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;

type EventHandler<T> = Box<dyn FnMut(T) -> BoxFuture<'static, ()> + Send + Sync>;

macro_rules! event_setter {
    ($fnc:ident, $event:ty) => {
        pub fn $fnc<F, Fut>(&self, mut callback: F)
        where
            F: FnMut($event) -> Fut + Send + Sync + 'static,
            Fut: Future<Output = ()> + Send + 'static,
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

#[derive(Clone, Debug)]
pub struct ParticipantConnectedEvent {
    pub room_session: RoomSession,
    pub participant: Arc<RemoteParticipant>,
}

#[derive(Clone, Debug)]
pub struct ParticipantDisconnectedEvent {
    pub room_session: RoomSession,
    pub participant: Arc<RemoteParticipant>,
}

#[derive(Clone, Debug)]
pub struct TrackSubscribedEvent {
    pub room_session: RoomSession,
    pub track: RemoteTrackHandle,
    pub publication: RemoteTrackPublication,
    pub participant: Arc<RemoteParticipant>,
}

#[derive(Clone, Debug)]
pub struct TrackPublishedEvent {
    pub room_session: RoomSession,
    pub publication: RemoteTrackPublication,
    pub participant: Arc<RemoteParticipant>,
}

#[derive(Clone, Debug)]
pub struct TrackSubscriptionFailedEvent {
    pub room_session: RoomSession,
    pub error: TrackError,
    pub sid: TrackSid,
    pub participant: Arc<RemoteParticipant>,
}

#[derive(Clone, Debug)]
pub struct ConnectionStateChangedEvent {
    pub room_session: RoomSession,
    pub state: ConnectionState,
}

pub(crate) type OnParticipantConnectedHandler = EventHandler<ParticipantConnectedEvent>;
pub(crate) type OnParticipantDisconnectedHandler = EventHandler<ParticipantDisconnectedEvent>;
pub(crate) type OnTrackSubscribedHandler = EventHandler<TrackSubscribedEvent>;
pub(crate) type OnTrackPublishedHandler = EventHandler<TrackPublishedEvent>;
pub(crate) type OnTrackSubscriptionFailedHandler = EventHandler<TrackSubscriptionFailedEvent>;
pub(crate) type OnConnectionStateChangedHandler = EventHandler<ConnectionStateChangedEvent>;

#[derive(Default)]
pub struct RoomEvents {
    pub(crate) on_participant_connected: Mutex<Option<OnParticipantConnectedHandler>>,
    pub(crate) on_participant_disconnected: Mutex<Option<OnParticipantDisconnectedHandler>>,
    pub(crate) on_track_subscribed: Mutex<Option<OnTrackSubscribedHandler>>,
    pub(crate) on_track_published: Mutex<Option<OnTrackPublishedHandler>>,
    pub(crate) on_track_subscription_failed: Mutex<Option<OnTrackSubscriptionFailedHandler>>,
    pub(crate) on_connection_state_changed: Mutex<Option<OnConnectionStateChangedHandler>>,
}

impl Debug for RoomEvents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RoomEvents")
    }
}

impl RoomEvents {
    event_setter!(on_participant_connected, ParticipantConnectedEvent);
    event_setter!(on_participant_disconnected, ParticipantDisconnectedEvent);
    event_setter!(on_track_subscribed, TrackSubscribedEvent);
    event_setter!(on_track_published, TrackPublishedEvent);
    event_setter!(on_track_subscription_failed, TrackSubscriptionFailedEvent);
    event_setter!(on_connection_state_changed, ConnectionStateChangedEvent);
}

#[derive(Default)]
pub struct ParticipantEvents {
    pub(crate) on_track_published: Mutex<Option<OnTrackPublishedHandler>>,
    pub(crate) on_track_subscribed: Mutex<Option<OnTrackSubscribedHandler>>,
    pub(crate) on_track_subscription_failed: Mutex<Option<OnTrackSubscriptionFailedHandler>>,
}

impl Debug for ParticipantEvents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ParticipantEvents")
    }
}

impl ParticipantEvents {
    event_setter!(on_track_published, TrackPublishedEvent);
    event_setter!(on_track_subscribed, TrackSubscribedEvent);
    event_setter!(on_track_subscription_failed, TrackSubscriptionFailedEvent);
}
