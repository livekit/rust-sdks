use crate::proto::{data_packet, DataPacket, UserPacket};
use crate::room::participant::{impl_participant_trait, ParticipantShared, ParticipantInternalTrait};
use crate::room::RoomError;
use crate::rtc_engine::RTCEngine;

pub struct LocalParticipant {
    shared: ParticipantShared,
    rtc_engine: Arc<RTCEngine>,
}

impl LocalParticipant {
    pub(crate) fn new(rtc_engine: Arc<RTCEngine>, info: ParticipantInfo) -> Self {
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

    pub(crate) async fn update_info(self: Arc<Self>, info: ParticipantInfo) {
        self.shared.update_info(info);
    }
}

impl ParticipantInternalTrait for LocalParticipant {
    fn internal_events(&self) -> Arc<ParticipantEvents> {
        self.shared.internal_events.clone()
    }
}

impl_participant_trait!(LocalParticipant);
