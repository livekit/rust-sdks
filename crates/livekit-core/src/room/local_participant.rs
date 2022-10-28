use crate::proto::{data_packet, DataPacket, UserPacket};
use crate::room::participant::{impl_participant_trait, ParticipantShared};
use crate::room::RoomError;
use crate::rtc_engine::RTCEngine;
use std::sync::Arc;

pub struct LocalParticipant {
    shared: ParticipantShared,
    rtc_engine: Arc<RTCEngine>,
}

impl LocalParticipant {
    pub(super) fn new(rtc_engine: Arc<RTCEngine>, info: ParticipantInfo) -> Self {
        Self {
            shared: ParticipantShared::new(
                info.sid.into(),
                info.identity.into(),
                info.name,
                info.metadata,
            ),
            rtc_engine,
        }
    }

    pub async fn publish_data(
        &self,
        data: &[u8],
        kind: data_packet::Kind,
    ) -> Result<(), RoomError> {
        let data = DataPacket {
            kind: kind as i32,
            value: Some(data_packet::Value::User(UserPacket {
                participant_sid: "".to_string(), /*self.sid().to_owned()*/
                payload: data.to_vec(),
                destination_sids: vec![],
            })),
        };

        self.rtc_engine
            .publish_data(&data, kind)
            .await
            .map_err(Into::into)
    }
}

impl_participant_trait!(LocalParticipant);
