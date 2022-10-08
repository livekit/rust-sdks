use std::sync::Arc;
use tokio::sync::Mutex;

use crate::proto::{data_packet, DataPacket, ParticipantInfo, UserPacket};
use crate::room::RoomError;
use crate::rtc_engine::RTCEngine;

pub struct LocalParticipant {
    sid: String,
    identity: String,
    name: String,

    engine: Arc<Mutex<RTCEngine>>,
}

impl LocalParticipant {
    pub(crate) fn from(info: ParticipantInfo, engine: Arc<Mutex<RTCEngine>>) -> Self {
        Self {
            sid: info.sid,
            identity: info.identity,
            name: info.name,
            engine,
        }
    }

    pub(crate) fn update(info: ParticipantInfo) {
        // TODO(theomonnom)
    }

    // TODO(theomonnom) Add the destinations parameter
    pub async fn publish_data(&mut self, data: &[u8], kind: data_packet::Kind) -> Result<(), RoomError> {
        let data = DataPacket {
            kind: kind as i32,
            value: Some(data_packet::Value::User(UserPacket {
                participant_sid: self.sid.clone(),
                payload: data.to_vec(),
                destination_sids: vec![], // TODO(theomonnom)
            })),
        };

        self.engine.lock().await.publish_data(&data, kind).await.map_err(Into::into)
    }
}
