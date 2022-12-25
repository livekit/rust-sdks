use std::sync::Weak;

use crate::proto::{data_packet, DataPacket, UserPacket};
use crate::room::participant::{
    impl_participant_trait, ParticipantInternalTrait, ParticipantShared,
};
use crate::room::RoomError;
use crate::rtc_engine::RTCEngine;

#[derive(Debug)]
pub struct LocalParticipant {
    shared: ParticipantShared,
    rtc_engine: Arc<RTCEngine>,
}

impl LocalParticipant {
    pub(crate) fn new(
        rtc_engine: Arc<RTCEngine>,
        sid: ParticipantSid,
        identity: ParticipantIdentity,
        name: String,
        metadata: String,
    ) -> Self {
        Self {
            shared: ParticipantShared::new(sid, identity, name, metadata),
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

impl ParticipantInternalTrait for LocalParticipant {
    fn internal_events(&self) -> Arc<ParticipantEvents> {
        self.shared.internal_events.clone()
    }

    fn update_info(&self, info: ParticipantInfo) {
        self.shared.update_info(info);
    }
}

impl_participant_trait!(LocalParticipant);
