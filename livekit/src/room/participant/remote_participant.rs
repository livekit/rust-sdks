use super::TrackKind;
use super::{ConnectionQuality, ParticipantInternal};
use crate::prelude::*;
use crate::rtc_engine::RtcEngine;
use crate::track::TrackError;
use livekit_protocol as proto;
use livekit_webrtc::prelude::*;
use parking_lot::{RwLock, RwLockReadGuard};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

const ADD_TRACK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Default)]
struct RemoteEvents {
    track_published: Option<Box<dyn Fn(RemoteTrackPublication)>>,
    track_unpublished: Option<Box<dyn Fn(RemoteTrackPublication)>>,
    track_subscribed: Option<Box<dyn Fn(RemoteTrack, RemoteTrackPublication)>>,
    track_unsubscribed: Option<Box<dyn Fn(RemoteTrack, RemoteTrackPublication)>>,
    track_subscription_failed: Option<Box<dyn Fn(TrackSid, TrackError)>>,
}

struct RemoteInfo {
    participant_inner: Arc<ParticipantInternal>,
    events: RwLock<RemoteEvents>,
}

#[derive(Clone)]
pub struct RemoteParticipant {
    inner: Arc<RemoteInfo>,
}

impl Debug for RemoteParticipant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteParticipant")
            .field("sid", &self.sid())
            .field("identity", &self.identity())
            .field("name", &self.name())
            .finish()
    }
}

impl RemoteParticipant {
    pub(crate) fn new(
        rtc_engine: Arc<RtcEngine>,
        sid: ParticipantSid,
        identity: ParticipantIdentity,
        name: String,
        metadata: String,
    ) -> Self {
        Self {
            inner: Arc::new(RemoteInfo {
                participant_inner: Arc::new(ParticipantInternal::new(
                    rtc_engine, sid, identity, name, metadata,
                )),
                events: RwLock::new(RemoteEvents::default()),
            }),
        }
    }

    pub(crate) async fn add_subscribed_media_track(
        &self,
        sid: TrackSid,
        media_track: MediaStreamTrack,
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

                    tokio::time::sleep(Duration::from_millis(50)).await;
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
            };

            log::debug!("starting track: {:?}", sid);

            remote_publication.update_track(Some(track.clone().into()));
            //track.set_muted(remote_publication.is_muted());
            track.update_info(proto::TrackInfo {
                sid: remote_publication.sid().to_string(),
                name: remote_publication.name().to_string(),
                r#type: proto::TrackType::from(remote_publication.kind()) as i32,
                source: proto::TrackSource::from(remote_publication.source()) as i32,
                ..Default::default()
            });

            self.inner
                .participant_inner
                .add_publication(TrackPublication::Remote(remote_publication.clone()));
            // track.start();
            track.enable();

            if let Some(track_subscribed) = self.inner.events.read().track_subscribed.as_ref() {
                track_subscribed(track, remote_publication);
            }
        } else {
            log::error!("could not find published track with sid: {:?}", sid);

            if let Some(track_subscription_failed) =
                self.inner.events.read().track_subscription_failed.as_ref()
            {
                track_subscription_failed(sid.clone(), TrackError::TrackNotFound(sid.0));
            }
        }
    }

    pub(crate) fn unpublish_track(&self, sid: &TrackSid) {
        if let Some(publication) = self.get_track_publication(sid) {
            // Unsubscribe to the track if needed
            if let Some(track) = publication.track() {
                track.disable();

                if let Some(track_unsubscribed) =
                    self.inner.events.read().track_unsubscribed.as_ref()
                {
                    track_unsubscribed(track.clone(), publication.clone());
                }
            }

            self.inner.participant_inner.remove_publication(sid);

            if let Some(track_unpublished) = self.inner.events.read().track_unpublished.as_ref() {
                track_unpublished(publication.clone());
            }

            publication.update_track(None);
        }
    }

    pub(crate) fn update_info(&self, info: proto::ParticipantInfo) {
        self.inner.participant_inner.update_info(info.clone());

        let mut valid_tracks = HashSet::<TrackSid>::new();
        for track in info.tracks {
            if let Some(publication) = self.get_track_publication(&track.sid.clone().into()) {
                publication.update_info(track.clone());
            } else {
                let publication = RemoteTrackPublication::new(
                    track.clone(),
                    Arc::downgrade(&self.inner.participant_inner),
                    None,
                );
                self.inner
                    .participant_inner
                    .add_publication(TrackPublication::Remote(publication.clone()));

                // This is a new track, dispatch publish event
                if let Some(track_published) = self.inner.events.read().track_published.as_ref() {
                    track_published(publication);
                }
            }

            valid_tracks.insert(track.sid.into());
        }

        // remove tracks that are no longer valid
        for (sid, _) in self.inner.participant_inner.tracks.read().iter() {
            if valid_tracks.contains(sid) {
                continue;
            }

            self.unpublish_track(sid);
        }
    }

    #[inline]
    pub fn get_track_publication(&self, sid: &TrackSid) -> Option<RemoteTrackPublication> {
        self.inner
            .participant_inner
            .tracks
            .read()
            .get(sid)
            .map(|track| {
                if let TrackPublication::Remote(remote) = track {
                    return remote.clone();
                }
                unreachable!()
            })
    }

    #[inline]
    pub fn sid(&self) -> ParticipantSid {
        self.inner.participant_inner.sid()
    }

    #[inline]
    pub fn identity(&self) -> ParticipantIdentity {
        self.inner.participant_inner.identity()
    }

    #[inline]
    pub fn name(&self) -> String {
        self.inner.participant_inner.name()
    }

    #[inline]
    pub fn metadata(&self) -> String {
        self.inner.participant_inner.metadata()
    }

    #[inline]
    pub fn is_speaking(&self) -> bool {
        self.inner.participant_inner.is_speaking()
    }

    #[inline]
    pub fn tracks(&self) -> RwLockReadGuard<HashMap<TrackSid, TrackPublication>> {
        self.inner.participant_inner.tracks()
    }

    #[inline]
    pub fn audio_level(&self) -> f32 {
        self.inner.participant_inner.audio_level()
    }

    #[inline]
    pub fn connection_quality(&self) -> ConnectionQuality {
        self.inner.participant_inner.connection_quality()
    }

    #[inline]
    pub(crate) fn set_speaking(&self, speaking: bool) {
        self.inner.participant_inner.set_speaking(speaking);
    }

    #[inline]
    pub(crate) fn set_audio_level(&self, level: f32) {
        self.inner.participant_inner.set_audio_level(level);
    }

    #[inline]
    pub(crate) fn set_connection_quality(&self, quality: ConnectionQuality) {
        self.inner.participant_inner.set_connection_quality(quality);
    }
}
