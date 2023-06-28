use super::ConnectionQuality;
use super::ParticipantInternal;
use crate::options;
use crate::options::compute_video_encodings;
use crate::options::video_layers_from_encodings;
use crate::options::TrackPublishOptions;
use crate::prelude::*;
use crate::rtc_engine::RtcEngine;
use crate::DataPacketKind;
use livekit_protocol as proto;
use livekit_webrtc::rtp_parameters::RtpEncodingParameters;
use parking_lot::{Mutex, RwLockReadGuard};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Default)]
struct LocalEvents {
    local_track_published: Mutex<Option<Box<dyn Fn(LocalTrackPublication) + Send>>>,
    local_track_unpublished: Mutex<Option<Box<dyn Fn(LocalTrackPublication) + Send>>>,
}

struct LocalInfo {
    events: LocalEvents,
}

#[derive(Clone)]
pub struct LocalParticipant {
    inner: Arc<ParticipantInternal>,
    local: Arc<LocalInfo>,
}

impl Debug for LocalParticipant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalParticipant")
            .field("sid", &self.sid())
            .field("identity", &self.identity())
            .field("name", &self.name())
            .finish()
    }
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
            inner: Arc::new(ParticipantInternal::new(
                rtc_engine, sid, identity, name, metadata,
            )),
            local: Arc::new(LocalInfo {
                events: LocalEvents::default(),
            }),
        }
    }

    pub async fn publish_track(
        &self,
        track: LocalTrack,
        options: TrackPublishOptions,
    ) -> RoomResult<LocalTrackPublication> {
        let mut req = proto::AddTrackRequest {
            cid: track.rtc_track().id(),
            name: track.name().clone(),
            r#type: proto::TrackType::from(track.kind()) as i32,
            muted: track.is_muted(),
            source: proto::TrackSource::from(options.source) as i32,
            disable_dtx: !options.dtx,
            disable_red: !options.red,
            ..Default::default()
        };

        let mut encodings = Vec::default();
        match &track {
            LocalTrack::Video(video_track) => {
                // Get the video dimension
                // TODO(theomonnom): Use MediaStreamTrack::getSettings() on web
                let resolution = video_track.rtc_source().video_resolution();
                req.width = resolution.width;
                req.height = resolution.height;

                encodings = compute_video_encodings(req.width, req.height, &options);
                req.layers = video_layers_from_encodings(req.width, req.height, &encodings);
            }
            LocalTrack::Audio(_audio_track) => {
                // Setup audio encoding
                let audio_encoding = options
                    .audio_encoding
                    .as_ref()
                    .unwrap_or(&options::audio::SPEECH.encoding);

                encodings.push(RtpEncodingParameters {
                    max_bitrate: Some(audio_encoding.max_bitrate),
                    ..Default::default()
                });
            }
        }
        let track_info = self.inner.rtc_engine.add_track(req).await?;
        let publication = LocalTrackPublication::new(
            track_info.clone(),
            Arc::downgrade(&self.inner),
            track.clone(),
        );
        track.update_info(track_info); // Update sid + source

        log::debug!("publishing track with cid {:?}", track.rtc_track().id());
        let transceiver = self
            .inner
            .rtc_engine
            .create_sender(track.clone(), options, encodings)
            .await?;

        track.set_transceiver(Some(transceiver));
        track.enable();

        tokio::spawn({
            let rtc_engine = self.inner.rtc_engine.clone();
            async move {
                let _ = rtc_engine.negotiate_publisher().await;
            }
        });

        self.inner
            .add_publication(TrackPublication::Local(publication.clone()));

        if let Some(local_track_published) = self.local.events.local_track_published.lock().as_ref()
        {
            local_track_published(publication.clone());
        }

        Ok(publication)
    }

    pub async fn unpublish_track(
        &self,
        track: TrackSid,
        _stop_on_unpublish: bool,
    ) -> RoomResult<LocalTrackPublication> {
        let mut tracks = self.inner.tracks.write();
        if let Some(TrackPublication::Local(publication)) = tracks.remove(&track) {
            let track = publication.track();
            let sender = track.transceiver().unwrap().sender();

            self.inner.rtc_engine.remove_track(sender).await?;
            track.set_transceiver(None);

            if let Some(local_track_unpublished) =
                self.local.events.local_track_unpublished.lock().as_ref()
            {
                local_track_unpublished(publication.clone());
            }

            //publication.set_track(None);

            tokio::spawn({
                let rtc_engine = self.inner.rtc_engine.clone();
                async move {
                    let _ = rtc_engine.negotiate_publisher().await;
                }
            });

            Ok(publication)
        } else {
            Err(RoomError::Internal("track not found".to_string()))
        }
    }

    pub async fn publish_data(
        &self,
        data: Vec<u8>,
        kind: DataPacketKind,
        destination_sids: Vec<String>,
    ) -> RoomResult<()> {
        let data = proto::DataPacket {
            kind: kind as i32,
            value: Some(proto::data_packet::Value::User(proto::UserPacket {
                payload: data,
                destination_sids: destination_sids.to_owned(),
                ..Default::default()
            })),
        };

        self.inner
            .rtc_engine
            .publish_data(&data, kind)
            .await
            .map_err(Into::into)
    }

    pub fn get_track_publication(&self, sid: &TrackSid) -> Option<LocalTrackPublication> {
        self.inner.tracks.read().get(sid).map(|track| {
            if let TrackPublication::Local(local) = track {
                return local.clone();
            }

            unreachable!()
        })
    }

    pub fn sid(&self) -> ParticipantSid {
        self.inner.info.read().sid.clone()
    }

    pub fn identity(&self) -> ParticipantIdentity {
        self.inner.info.read().identity.clone()
    }

    pub fn name(&self) -> String {
        self.inner.info.read().name.clone()
    }

    pub fn metadata(&self) -> String {
        self.inner.info.read().metadata.clone()
    }

    pub fn is_speaking(&self) -> bool {
        self.inner.info.read().speaking
    }

    pub fn tracks(&self) -> RwLockReadGuard<HashMap<TrackSid, TrackPublication>> {
        self.inner.tracks.read()
    }

    pub fn audio_level(&self) -> f32 {
        self.inner.info.read().audio_level
    }

    pub fn connection_quality(&self) -> ConnectionQuality {
        self.inner.info.read().connection_quality
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
