use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use self::id::{ParticipantIdentity, ParticipantSid};
use self::internal::{RoomInternal, RoomSession};
use self::participant::local_participant::LocalParticipant;
use self::participant::remote_participant::RemoteParticipant;
use self::participant::ParticipantInternalTrait;
use self::participant::ParticipantTrait;
use crate::events::{
    ParticipantConnectedEvent, ParticipantDisconnectedEvent, RoomEvents, TrackPublishedEvent,
    TrackSubscribedEvent,
};
use crate::proto;
use crate::proto::participant_info;
use thiserror::Error;
use tracing::{debug, error, instrument, trace_span, Level};

use crate::rtc_engine::{EngineError, EngineEvent, EngineEvents, EngineResult, RTCEngine};
use crate::signal_client::SignalOptions;

pub use crate::rtc_engine::SimulateScenario;

pub mod id;
mod internal;
pub mod participant;
pub mod publication;
pub mod track;

#[derive(Error, Debug)]
pub enum RoomError {
    #[error("engine : {0}")]
    Engine(#[from] EngineError),
    #[error("room failure: {0}")]
    Internal(String),
}

pub type RoomResult<T> = Result<T, RoomError>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

#[derive(Clone, Debug)]
pub struct RoomHandle {
    session: Arc<RoomSession>,
}

impl RoomHandle {
    pub fn sid(&self) -> String {
        self.session.sid.lock().clone()
    }

    pub fn name(&self) -> String {
        self.internal.name.lock().clone()
    }

    pub fn local_participant(&self) -> Arc<LocalParticipant> {
        self.internal.local_participant.clone()
    }

    pub async fn simulate_scenario(&self, scenario: SimulateScenario) -> EngineResult<()> {
        self.internal.rtc_engine.simulate_scenario(scenario).await
    }
}

#[derive(Debug, Default)]
pub struct Room {
    session: Option<RoomHandle>,
    events: Arc<RoomEvents>, // Keep the same RoomEvents across sessions
}

impl Room {
    #[instrument(level = Level::DEBUG)]
    pub async fn connect(&self, url: &str, token: &str) -> RoomResult<()> {
        let room_session = Arc::new(RoomSession::connect(self.events.clone(), url, token).await?);
        self.session = Some(room_session.clone());
        Ok(())
    }

    pub async fn close(&self) {}

    pub fn events(&self) -> Arc<RoomEvents> {
        self.events.clone()
    }

    pub fn get_handle(&self) -> Option<RoomHandle> {
        self.internal.as_ref().map(|internal| RoomHandle {
            internal: internal.clone(),
        })
    }
}
