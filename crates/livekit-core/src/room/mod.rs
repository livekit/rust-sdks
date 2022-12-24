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

mod room_session;
pub mod id;
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
pub struct RoomSession {
    internal: Arc<RoomInternal>,
}

impl RoomSession {
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
    session: Option<Arc<RoomSession>>,
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

    pub fn session(&self) -> Option<> {
        self.internal.as_ref().map(|internal| RoomHandle {
            internal: internal.clone(),
        })
    }
}

#[derive(Debug)]
pub struct RoomInternal {
    inner: Arc<RoomInner>,
    session_task: JoinHandle<()>,
    close_emitter: oneshot::Sender<()>,
}

impl RoomInternal {
    pub async fn connect(room_events: Arc<RoomEvents>, url: &str, token: &str) -> RoomResult<Self> {
        let (rtc_engine, engine_events) = RTCEngine::new();
        let rtc_engine = Arc::new(rtc_engine);
        rtc_engine
            .connect(url, token, SignalOptions::default())
            .await?;

        let join_response = rtc_engine.join_response().unwrap();
        let pi = join_response.participant.unwrap().clone();
        let local_participant = Arc::new(LocalParticipant::new(
            rtc_engine.clone(),
            pi.sid.into(),
            pi.identity.into(),
            pi.name,
            pi.metadata,
        ));
        let room_info = join_response.room.unwrap();
        let inner = Arc::new(SessionInner {
            state: AtomicU8::new(ConnectionState::Connecting as u8),
            sid: Mutex::new(room_info.sid),
            name: Mutex::new(room_info.name),
            participants: Default::default(),
            rtc_engine,
            local_participant,
            room_events,
        });

        for pi in join_response.other_participants {
            let participant = {
                let pi = pi.clone();
                inner.create_participant(pi.sid.into(), pi.identity.into(), pi.name, pi.metadata)
            };
            participant.update_info(pi.clone());
            participant
                .update_tracks(RoomHandle::from(inner.clone()), pi.tracks)
                .await;
        }

        let (close_emitter, close_receiver) = oneshot::channel();
        let session_task = tokio::spawn(inner.room_task(engine_events, close_receiver));

        let session = Self {
            inner,
            session_task,
            close_emitter,
        };
        Ok(session)
    }

    pub async fn close(self) {
        self.inner.close();
        let _ = self.close_emitter.send(());
        self.session_task.await;
    }
}
