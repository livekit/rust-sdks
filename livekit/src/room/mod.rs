use crate::participant::ConnectionQuality;
use crate::prelude::*;
use crate::proto;
use crate::rtc_engine::EngineError;
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;

pub use crate::rtc_engine::SimulateScenario;

pub mod id;
pub mod participant;
pub mod publication;
pub mod room_session;
pub mod track;

pub use room_session::*;

pub type RoomResult<T> = Result<T, RoomError>;

#[derive(Error, Debug)]
pub enum RoomError {
    #[error("engine : {0}")]
    Engine(#[from] EngineError),
    #[error("room failure: {0}")]
    Internal(String),
}

#[derive(Clone, Debug)]
pub enum RoomEvent {
    ParticipantConnected(Arc<RemoteParticipant>),
    ParticipantDisconnected(Arc<RemoteParticipant>),
    TrackSubscribed {
        track: RemoteTrackHandle,
        publication: RemoteTrackPublication,
        participant: Arc<RemoteParticipant>,
    },
    TrackPublished {
        publication: RemoteTrackPublication,
        participant: Arc<RemoteParticipant>,
    },
    TrackUnpublished {
        publication: RemoteTrackPublication,
        participant: Arc<RemoteParticipant>,
    },
    TrackUnsubscribed {
        track: RemoteTrackHandle,
        publication: RemoteTrackPublication,
        participant: Arc<RemoteParticipant>,
    },
    TrackSubscriptionFailed {
        error: track::TrackError,
        sid: TrackSid,
        participant: Arc<RemoteParticipant>,
    },
    TrackMuted {
        participant: Participant,
        publication: TrackPublication,
    },
    TrackUnmuted {
        participant: Participant,
        publication: TrackPublication,
    },
    ActiveSpeakersChanged {
        speakers: Vec<Participant>,
    },
    ConnectionQualityChanged {
        quality: ConnectionQuality,
        participant: Participant,
    },
    DataReceived {
        payload: Arc<Vec<u8>>,
        kind: proto::data_packet::Kind,
        participant: Arc<RemoteParticipant>,
    },
    ConnectionStateChanged(ConnectionState),
    Connected,
    Disconnected,
    Reconnecting,
    Reconnected,
}

#[derive(Debug)]
pub struct Room {
    handle: SessionHandle,
}

impl Room {
    pub async fn connect(
        url: &str,
        token: &str,
    ) -> RoomResult<(Self, mpsc::UnboundedReceiver<RoomEvent>)> {
        let handle = SessionHandle::connect(url, token).await?;
        let events = handle.subscribe();
        Ok((Self { handle }, events))
    }

    pub async fn close(self) {
        self.handle.close().await;
    }

    /// Allow multiple subscribers/observers to receive events
    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<RoomEvent> {
        self.handle.subscribe()
    }

    pub fn session(&self) -> RoomSession {
        self.handle.session()
    }
}
