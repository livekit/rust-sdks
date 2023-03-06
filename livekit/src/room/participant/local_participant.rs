use super::{ConnectionQuality, ParticipantInner};
use crate::options::compute_video_encodings;
use crate::options::TrackPublishOptions;
use crate::prelude::*;
use crate::proto;
use crate::rtc_engine::RtcEngine;
use crate::track::video_layers_from_encodings;
use parking_lot::RwLockReadGuard;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct LocalParticipant {
    inner: Arc<ParticipantInner>,
    rtc_engine: Arc<RtcEngine>,
}

impl LocalParticipant {
    pub(crate) fn new(
        rtc_engine: Arc<RtcEngine>,
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

    pub async fn publish_track(
        &self,
        track: LocalTrack,
        options: TrackPublishOptions,
    ) -> RoomResult<LocalTrackPublication> {
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

        let mut req = proto::AddTrackRequest {
            cid: track.rtc_track().id(),
            name: options.name.clone(),
            r#type: proto::TrackType::from(track.kind()) as i32,
            muted: track.muted(),
            source: proto::TrackSource::from(track.source()) as i32,
            disable_dtx: !options.dtx,
            disable_red: !options.red,
            ..Default::default()
        };

        match &track {
            LocalTrack::Video(video_track) => {
                // Get the video dimension
                // TODO(theomonnom): Use MediaStreamTrack::getSettings() on web
                let capture_options = video_track.capture_options();
                req.width = capture_options.preset.width;
                req.height = capture_options.preset.height;

                let encodings = compute_video_encodings(req.width, req.height, &options);
                req.layers = video_layers_from_encodings(req.width, req.height, &encodings);
            }
            LocalTrack::Audio(_audio_track) => {}
        }

        let track_info = self.rtc_engine.add_track(req).await?;
        let publication = LocalTrackPublication::new(track_info.clone(), track.clone(), options);
        track.update_info(track_info); // Update SID

        tokio::spawn({
            // Renegotiate in background
            let rtc_engine = self.rtc_engine.clone();
            async move {
                let _ = rtc_engine.negotiate_publisher().await;
            }
        });

        self.inner
            .add_track_publication(TrackPublication::Local(publication.clone()));
        Ok(publication)
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

    #[inline]
    pub fn get_track_publication(&self, sid: &TrackSid) -> Option<LocalTrackPublication> {
        self.inner.tracks.read().get(sid).map(|track| {
            if let TrackPublication::Local(local) = track {
                return local.clone();
            }

            unreachable!()
        })
    }

    #[inline]
    pub fn sid(&self) -> ParticipantSid {
        self.inner.sid()
    }

    #[inline]
    pub fn identity(&self) -> ParticipantIdentity {
        self.inner.identity()
    }

    #[inline]
    pub fn name(&self) -> String {
        self.inner.name()
    }

    #[inline]
    pub fn metadata(&self) -> String {
        self.inner.metadata()
    }

    #[inline]
    pub fn is_speaking(&self) -> bool {
        self.inner.is_speaking()
    }

    #[inline]
    pub fn tracks(&self) -> RwLockReadGuard<HashMap<TrackSid, TrackPublication>> {
        self.inner.tracks()
    }

    #[inline]
    pub fn audio_level(&self) -> f32 {
        self.inner.audio_level()
    }

    #[inline]
    pub fn connection_quality(&self) -> ConnectionQuality {
        self.inner.connection_quality()
    }

    #[inline]
    pub fn register_observer(&self) -> mpsc::UnboundedReceiver<ParticipantEvent> {
        self.inner.register_observer()
    }

    #[inline]
    pub(crate) fn update_info(self: &Self, info: proto::ParticipantInfo) {
        self.inner.update_info(info);
    }

    #[inline]
    pub(crate) fn set_speaking(&self, speaking: bool) {
        self.inner.set_speaking(speaking);
    }

    #[inline]
    pub(crate) fn set_audio_level(&self, level: f32) {
        self.inner.set_audio_level(level);
    }

    #[inline]
    pub(crate) fn set_connection_quality(&self, quality: ConnectionQuality) {
        self.inner.set_connection_quality(quality);
    }
}
