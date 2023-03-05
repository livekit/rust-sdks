use super::{ConnectionQuality, ParticipantInner};
use crate::prelude::*;
use crate::proto;
use crate::track::TrackError;
use livekit_webrtc as rtc;
use parking_lot::RwLockReadGuard;
use rtc::prelude::MediaStreamTrack;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::{debug, error, instrument, Level};

const ADD_TRACK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct RemoteParticipant {
    inner: Arc<ParticipantInner>,
}

impl RemoteParticipant {
    pub(crate) fn new(
        sid: ParticipantSid,
        identity: ParticipantIdentity,
        name: String,
        metadata: String,
    ) -> Self {
        Self {
            inner: Arc::new(ParticipantInner::new(sid, identity, name, metadata)),
        }
    }

    #[inline]
    pub fn get_track_publication(&self, sid: &TrackSid) -> Option<RemoteTrackPublication> {
        self.inner.tracks.read().get(sid).map(|track| {
            if let TrackPublication::Remote(remote) = track {
                return remote.clone();
            }
            unreachable!()
        })
    }

    /// Called by the RoomSession when receiving data from the RrcSession
    /// It is just used to emit the Data event on the participant dispatcher.
    pub(crate) fn on_data_received(&self, data: Arc<Vec<u8>>, kind: proto::data_packet::Kind) {
        self.inner
            .dispatcher
            .lock()
            .dispatch(&ParticipantEvent::DataReceived {
                payload: data,
                kind,
            });
    }

    #[instrument(level = Level::DEBUG)]
    pub(crate) async fn add_subscribed_media_track(
        &self,
        sid: TrackSid,
        media_track: rtc::media_stream::MediaStreamTrack,
    ) {
        let wait_publication = {
            let participant = self.clone();
            let sid = sid.clone();
            async move {
                loop {
                    let publication = participant.get_track_publication(&sid);
                    if let Some(publication) = publication {
                        return publication;
                    }

                    tokio::task::yield_now().await;
                }
            }
        };

        if let Ok(remote_publication) = timeout(ADD_TRACK_TIMEOUT, wait_publication).await {
            let track = match remote_publication.kind() {
                TrackKind::Audio => {
                    if let MediaStreamTrack::Audio(rtc_track) = media_track {
                        let audio_track = RemoteAudioTrack::new(
                            remote_publication.sid().into(),
                            remote_publication.name(),
                            rtc_track,
                        );
                        RemoteTrack::Audio(audio_track)
                    } else {
                        unreachable!();
                    }
                }
                TrackKind::Video => {
                    if let MediaStreamTrack::Video(rtc_track) = media_track {
                        let video_track = RemoteVideoTrack::new(
                            remote_publication.sid().into(),
                            remote_publication.name(),
                            rtc_track,
                        );
                        RemoteTrack::Video(video_track)
                    } else {
                        unreachable!()
                    }
                }
                _ => unreachable!(),
            };

            debug!("starting track: {:?}", sid);

            remote_publication.update_track(Some(track.clone().into()));
            track.set_muted(remote_publication.muted());
            track.set_source(remote_publication.source());

            self.inner
                .add_track_publication(TrackPublication::Remote(remote_publication.clone()));
            track.start();

            self.inner
                .dispatcher
                .lock()
                .dispatch(&ParticipantEvent::TrackSubscribed {
                    track,
                    publication: remote_publication,
                });
        } else {
            error!("could not find published track with sid: {:?}", sid);

            self.inner
                .dispatcher
                .lock()
                .dispatch(&ParticipantEvent::TrackSubscriptionFailed {
                    sid: sid.clone(),
                    error: TrackError::TrackNotFound(sid.clone().to_string()),
                });
        }
    }

    pub(crate) fn unpublish_track(&self, sid: &TrackSid) {
        if let Some(publication) = self.get_track_publication(sid) {
            // Unsubscribe to the track if needed
            if let Some(track) = publication.track() {
                track.stop();

                self.inner
                    .dispatcher
                    .lock()
                    .dispatch(&ParticipantEvent::TrackUnsubscribed {
                        track: track.clone(),
                        publication: publication.clone(),
                    });
            }

            self.inner
                .dispatcher
                .lock()
                .dispatch(&ParticipantEvent::TrackUnpublished {
                    publication: publication.clone(),
                });

            publication.update_track(None);
        }
    }

    pub(crate) fn update_info(&self, info: proto::ParticipantInfo) {
        self.inner.update_info(info.clone());

        let mut valid_tracks = HashSet::<TrackSid>::new();
        for track in info.tracks {
            if let Some(publication) = self.get_track_publication(&track.sid.clone().into()) {
                publication.update_info(track.clone());
            } else {
                let publication = RemoteTrackPublication::new(track.clone(), self.sid(), None);
                self.inner
                    .add_track_publication(TrackPublication::Remote(publication.clone()));

                // This is a new track, dispatch publish event
                self.inner
                    .dispatcher
                    .lock()
                    .dispatch(&ParticipantEvent::TrackPublished { publication });
            }

            valid_tracks.insert(track.sid.into());
        }

        // remove tracks that are no longer valid
        for (sid, _) in self.inner.tracks.read().iter() {
            if valid_tracks.contains(sid) {
                continue;
            }

            self.unpublish_track(sid);
        }
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
