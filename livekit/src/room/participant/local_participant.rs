use crate::options::TrackPublishOptions;
use crate::prelude::*;
use crate::proto;
use crate::rtc_engine::RTCEngine;
use futures::channel::mpsc;
use parking_lot::RwLockReadGuard;
use std::collections::HashMap;
use std::sync::Arc;

use super::ConnectionQuality;
use super::ParticipantInner;

#[derive(Debug, Clone)]
pub struct LocalParticipant {
    inner: Arc<ParticipantInner>,
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
            inner: Arc::new(ParticipantInner::new(sid, identity, name, metadata)),
            rtc_engine,
        }
    }

    pub fn sid(&self) -> ParticipantSid {
        self.inner.sid()
    }

    pub fn identity(&self) -> ParticipantIdentity {
        self.inner.identity()
    }

    pub fn name(&self) -> String {
        self.inner.name()
    }

    pub fn metadata(&self) -> String {
        self.inner.metadata()
    }

    pub fn is_speaking(&self) -> bool {
        self.inner.is_speaking()
    }

    pub fn tracks(&self) -> RwLockReadGuard<HashMap<TrackSid, TrackPublication>> {
        self.inner.tracks()
    }

    pub fn audio_level(&self) -> f32 {
        self.inner.audio_level()
    }

    pub fn connection_quality(&self) -> ConnectionQuality {
        self.inner.connection_quality()
    }

    pub fn register_observer(&self) -> mpsc::UnboundedReceiver<ParticipantEvent> {
        self.inner.register_observer()
    }

    pub fn get_track_publication(&self, sid: &TrackSid) -> Option<LocalTrackPublication> {
        self.inner.tracks.read().get(sid).map(|track| {
            if let TrackPublication::Local(local) = track {
                return local.clone();
            }
            unreachable!()
        })
    }

    pub async fn publish_track(
        &self,
        track: LocalTrackHandle,
        options: TrackPublishOptions,
    ) -> RoomResult<()> {
        let tracks = self.inner.tracks.write();

        if track.source() != TrackSource::Unknown {
            for publication in tracks.values() {
                if publication.source() == track.source() {
                    return Err(RoomError::TrackAlreadyPublished);
                }

                if let Some(existing_track) = publication.track() {
                    // TODO: Compare
                }
            }
        }

        let req = proto::AddTrackRequest {};

        Ok(())
    }

    pub async fn publish_data(
        &self,
        data: &[u8],
        kind: proto::data_packet::Kind,
    ) -> Result<(), RoomError> {
        let data = proto::DataPacket {
            kind: kind as i32,
            value: Some(proto::data_packet::Value::User(proto::UserPacket {
                participant_sid: self.sid().to_string(),
                payload: data.to_vec(),
                destination_sids: vec![],
            })),
        };

        self.rtc_engine
            .publish_data(&data, kind)
            .await
            .map_err(Into::into)
    }

    pub(crate) fn update_info(self: &Self, info: proto::ParticipantInfo) {
        self.inner.update_info(info);
    }

    pub(crate) fn set_speaking(&self, speaking: bool) {
        self.inner.set_speaking(speaking);
    }

    pub(crate) fn set_audio_level(&self, level: f32) {
        self.inner.set_audio_level(level);
    }

    pub(crate) fn set_connection_quality(&self, quality: ConnectionQuality) {
        self.inner.set_connection_quality(quality);
    }
}
