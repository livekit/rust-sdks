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

pub struct Room {
    sid: String,
    name: String,
    local_participant: LocalParticipant,
    engine: Arc<Mutex<RTCEngine>>,
}

#[tracing::instrument(skip(url, token))]
pub async fn connect(url: &str, token: &str) -> Result<Room, RoomError> {
    let engine = rtc_engine::connect(url, token).await?;

    engine.on_data(Box::new(|packet| {
        Box::pin(async move {})
    })).await;

    let join = engine.join_response().await;
    let engine = Arc::new(Mutex::new(engine));
    let lp = LocalParticipant::from(join.participant.unwrap(), engine.clone());

    let room_info = join.room.unwrap();
    Ok(Room {
        sid: room_info.sid,
        name: room_info.name,
        local_participant: lp,
        engine,
    })
}

impl Room {
    pub fn local_participant(&mut self) -> &mut LocalParticipant {
        &mut self.local_participant
    }

    pub fn sid(&self) -> &str {
        &self.sid
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
