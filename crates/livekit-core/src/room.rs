use std::sync::Arc;
use std::time::Duration;
use log::trace;

use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::local_participant::LocalParticipant;
use crate::proto::data_packet;
use crate::proto::signal_request::Message::Mute;
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

pub async fn connect(url: &str, token: &str) -> Result<Room, RoomError> {
    let engine = rtc_engine::connect(url, token).await?;

    engine.on_data(Box::new(|packet| {
        trace!("This is a test");

        Box::pin(async move {
            trace!("Another test");
        })
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


#[tokio::test]
async fn test_test() {
    env_logger::init();
    let mut room = connect("ws://localhost:7880", "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE2NzEyMzk4NjAsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJ0ZXN0IiwibmJmIjoxNjY0MDM5ODYwLCJzdWIiOiJ0ZXN0IiwidmlkZW8iOnsicm9vbUFkbWluIjp0cnVlLCJyb29tQ3JlYXRlIjp0cnVlLCJyb29tSm9pbiI6dHJ1ZX19.0Bee2jI2cSZveAbZ8MLc-ADoMYQ4l8IRxcAxpXAS6a8").await.unwrap();
    room.local_participant().publish_data(b"This is a test", data_packet::Kind::Reliable).await.unwrap();
    //sleep(Duration::from_secs(60)).await;
}
