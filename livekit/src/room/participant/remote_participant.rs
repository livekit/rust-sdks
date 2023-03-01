use super::ConnectionQuality;
use super::ParticipantInner;
use crate::prelude::*;
use crate::proto;
use crate::publication::TrackPublicationInternalTrait;
use crate::track::TrackError;
use livekit_webrtc as rtc;
use parking_lot::RwLockReadGuard;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, error, instrument, Level};

const ADD_TRACK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct RemoteParticipant {
    shared: Arc<ParticipantInner>,
}

impl RemoteParticipant {
    pub(crate) fn new(
        sid: ParticipantSid,
        identity: ParticipantIdentity,
        name: String,
        metadata: String,
    ) -> Self {
        Self {
            shared: Arc::new(ParticipantInner::new(sid, identity, name, metadata)),
        }
    }

    fn get_track_publication(&self, sid: &TrackSid) -> Option<RemoteTrackPublication> {
        self.shared.tracks.read().get(sid).map(|track| {
            if let TrackPublication::Remote(remote) = track {
                remote.clone()
            } else {
                unreachable!()
            }
        })
    }

    /// Called by the RoomSession when receiving data by the RTCSession
    /// It is just used to emit the Data event on the participant dispatcher.
    pub(crate) fn on_data_received(&self, data: Arc<Vec<u8>>, kind: proto::data_packet::Kind) {
        self.shared
            .dispatcher
            .lock()
            .dispatch(&ParticipantEvent::DataReceived {
                payload: data,
                kind,
            });
    }

    #[instrument(level = Level::DEBUG)]
    pub(crate) async fn add_subscribed_media_track(
        self: Arc<Self>,
        sid: TrackSid,
        media_track: MediaStreamTrackHandle,
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
                    if let MediaStreamTrackHandle::Audio(rtc_track) = media_track {
                        let audio_track = RemoteAudioTrack::new(
                            remote_publication.sid().into(),
                            remote_publication.name(),
                            rtc_track,
                        );
                        RemoteTrackHandle::Audio(Arc::new(audio_track))
                    } else {
                        unreachable!();
                    }
                }
                TrackKind::Video => {
                    if let MediaStreamTrackHandle::Video(rtc_track) = media_track {
                        let video_track = RemoteVideoTrack::new(
                            remote_publication.sid().into(),
                            remote_publication.name(),
                            rtc_track,
                        );
                        RemoteTrackHandle::Video(Arc::new(video_track))
                    } else {
                        unreachable!()
                    }
                }
                _ => unreachable!(),
            };

            debug!("starting track: {:?}", sid);

            remote_publication.update_track(Some(track.clone().into()));
            track.update_muted(remote_publication.muted(), false);
            track.update_source(remote_publication.source());

            self.shared
                .add_track_publication(TrackPublication::Remote(remote_publication.clone()));
            track.start();

            self.shared
                .dispatcher
                .lock()
                .dispatch(&ParticipantEvent::TrackSubscribed {
                    track: track,
                    publication: remote_publication,
                });
        } else {
            error!("could not find published track with sid: {:?}", sid);

            self.shared
                .dispatcher
                .lock()
                .dispatch(&ParticipantEvent::TrackSubscriptionFailed {
                    sid: sid.clone(),
                    error: TrackError::TrackNotFound(sid.clone().to_string()),
                });
        }
    }

    pub(crate) fn unpublish_track(self: &Arc<Self>, sid: &TrackSid, emit_events: bool) {
        if let Some(publication) = self.get_track_publication(sid) {
            // Unsubscribe to the track if needed
            if let Some(track) = publication.track() {
                track.stop();

                self.shared
                    .dispatcher
                    .lock()
                    .dispatch(&ParticipantEvent::TrackUnsubscribed {
                        track: track.clone(),
                        publication: publication.clone(),
                    });
            }

            if emit_events {
                self.shared
                    .dispatcher
                    .lock()
                    .dispatch(&ParticipantEvent::TrackUnpublished {
                        publication: publication.clone(),
                    });
            }

            publication.update_track(None);
        }
    }

    pub(crate) fn update_info(self: &Arc<Self>, info: proto::ParticipantInfo, emit_events: bool) {
        self.shared.update_info(info.clone());

        let mut valid_tracks = HashSet::<TrackSid>::new();
        for track in info.tracks {
            if let Some(publication) = self.get_track_publication(&track.sid.clone().into()) {
                publication.update_info(track.clone());
            } else {
                let publication = RemoteTrackPublication::new(track.clone(), self.sid(), None);
                self.shared
                    .add_track_publication(TrackPublication::Remote(publication.clone()));

                // This is a new track, dispatch publish event
                if emit_events {
                    self.shared
                        .dispatcher
                        .lock()
                        .dispatch(&ParticipantEvent::TrackPublished { publication });
                }
            }

            valid_tracks.insert(track.sid.into());
        }

        // remove tracks that are no longer valid
        for (sid, _) in self.shared.tracks.read().iter() {
            if valid_tracks.contains(sid) {
                continue;
            }

            self.unpublish_track(sid, emit_events);
        }
    }

    pub(crate) fn set_speaking(&self, speaking: bool) {
        self.shared.set_speaking(speaking);
    }

    pub(crate) fn set_audio_level(&self, level: f32) {
        self.shared.set_audio_level(level);
    }

    pub(crate) fn set_connection_quality(&self, quality: ConnectionQuality) {
        self.shared.set_connection_quality(quality);
    }
}
