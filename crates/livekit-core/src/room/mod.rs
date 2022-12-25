use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use self::id::{ParticipantIdentity, ParticipantSid};
use self::participant::local_participant::LocalParticipant;
use self::participant::remote_participant::RemoteParticipant;
use self::participant::ParticipantInternalTrait;
use self::participant::ParticipantTrait;
use self::room_session::{RoomInternal, RoomSession};
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
pub mod participant;
pub mod publication;
pub mod room_session;
pub mod track;

#[derive(Error, Debug)]
pub enum RoomError {
    #[error("engine : {0}")]
    Engine(#[from] EngineError),
    #[error("room failure: {0}")]
    Internal(String),
}

pub type RoomResult<T> = Result<T, RoomError>;

#[derive(Debug, Default)]
pub struct Room {
    internal: Option<RoomInternal>,
    events: Arc<RoomEvents>, // Keep the same RoomEvents across sessions
}

impl Room {
    #[instrument(level = Level::DEBUG)]
    pub async fn connect(&mut self, url: &str, token: &str) -> RoomResult<()> {
        let internal = RoomInternal::connect(self.events.clone(), url, token).await?;
        self.internal = Some(internal);
        Ok(())
    }

    #[instrument(level = Level::DEBUG)]
    pub async fn close(&mut self) {
        if let Some(internal) = self.internal.take() {
            internal.close().await;
        }
    }

    pub fn events(&self) -> Arc<RoomEvents> {
        self.events.clone()
    }

    pub fn session(&self) -> Option<RoomSession> {
        self.internal.as_ref().map(RoomInternal::session)
    }
}
