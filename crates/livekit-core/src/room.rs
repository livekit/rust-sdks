use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex;

use crate::local_participant::LocalParticipant;
use crate::rtc_engine;
use crate::rtc_engine::{EngineError, RTCEngine};

#[derive(Error, Debug)]
pub enum RoomError {
    #[error("internal RTCEngine failure")]
    Engine(#[from] EngineError),
}

#[derive(Debug)]
pub enum RoomEvent { 
    
}

pub struct Room {
    sid: String,
    name: String,
    local_participant: LocalParticipant,
    internal: Arc<RoomInternal>
}

#[tracing::instrument(skip(url, token))]
pub async fn connect(url: &str, token: &str) -> Result<Room, RoomError> {
    let engine = rtc_engine::connect(url, token).await?;
    let join = engine.join_response().await;
    let engine = Arc::new(Mutex::new(engine));
    let local_participant = LocalParticipant::from(join.participant.unwrap(), engine.clone());
    let internal = Arc::new(RoomInternal::new(engine));
    let room_info = join.room.unwrap();

    tokio::spawn(async move {

    });

    Ok(Room {
        sid: room_info.sid,
        name: room_info.name,
        local_participant,
        internal,
    })
}

impl Room {
    pub fn local_participant(&self) -> &LocalParticipant {
        &self.local_participant
    }

    pub fn local_participant_mut(&mut self) -> &mut LocalParticipant {
        &mut self.local_participant
    }

    pub fn sid(&self) -> &str {
        &self.sid
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}


struct RoomInternal {
    engine: Arc<Mutex<RTCEngine>>,
}

impl RoomInternal {
    pub fn new(engine: Arc<Mutex<RTCEngine>>) -> Self {
        Self {
            engine
        }
    }
}