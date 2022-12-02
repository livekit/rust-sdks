use crate::events::participant::{
    TrackPublishedEvent, TrackSubscribedEvent, TrackSubscriptionFailedEvent,
};
use crate::events::TrackError;
use crate::room::id::TrackSid;
use crate::room::participant::{
    impl_participant_trait, ParticipantInternalTrait, ParticipantShared,
};
use crate::room::publication::{
    RemoteTrackPublication, TrackPublication, TrackPublicationInternalTrait, TrackPublicationTrait,
};
use crate::room::track::remote_audio_track::RemoteAudioTrack;
use crate::room::track::remote_track::RemoteTrackHandle;
use crate::room::track::remote_video_track::RemoteVideoTrack;
use crate::room::track::{TrackKind, TrackTrait, TrackHandle};
use livekit_webrtc::media_stream::MediaStreamTrackHandle;
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::{info, error};

use super::ParticipantTrait;

const ADD_TRACK_TIMEOUT: Duration = Duration::from_secs(5);

pub struct RemoteParticipant {
    shared: ParticipantShared,
}

impl RemoteParticipant {
    pub(crate) fn new(info: ParticipantInfo) -> Self {
        Self {
            shared: ParticipantShared::new(
                info.sid.into(),
                info.identity.into(),
                info.name,
                info.metadata,
            ),
        }
    }

    pub(crate) fn add_subscribed_media_track(
        self: Arc<Self>,
        sid: TrackSid,
        media_track: MediaStreamTrackHandle,
    ) {
        tokio::spawn(async move {
            let wait_publication = {
                let participant = self.clone();
                let sid = sid.clone();
                async move {
                    loop {
                        let publication = participant.get_track_publication(&sid);
                        if let Some(publication) = publication {
                            return publication;
                        }

                        sleep(Duration::from_millis(50)).await;
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

                info!("starting track: {:?}", sid);

                remote_publication.update_track(Some(track.clone().into()));
                self.shared
                    .add_track_publication(TrackPublication::Remote(remote_publication.clone()));
                track.start();

                let event = TrackSubscribedEvent {
                    track,
                    publication: remote_publication,
                    participant: self.clone(),
                };

                if let Some(cb) = self
                    .shared
                    .internal_events
                    .on_track_subscribed
                    .lock()
                    .as_mut()
                {
                    cb(event.clone()).await;
                }

                if let Some(cb) = self.shared.events.on_track_subscribed.lock().as_mut() {
                    cb(event).await;
                }
            } else {
                error!("could not find published track with sid: {:?}", sid);

                let event = TrackSubscriptionFailedEvent {
                    sid: sid.clone(),
                    error: TrackError::TrackNotFound(sid.clone().to_string()),
                    participant: self.clone(),
                };

                if let Some(cb) = self
                    .shared
                    .internal_events
                    .on_track_subscription_failed
                    .lock()
                    .as_mut()
                {
                    cb(event.clone()).await;
                }

                if let Some(cb) = self
                    .shared
                    .events
                    .on_track_subscription_failed
                    .lock()
                    .as_mut()
                {
                    cb(event).await;
                }
            }
        });
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

    pub(crate) async fn update_info(self: Arc<Self>, info: ParticipantInfo) {
        self.shared.update_info(info.clone());

        let mut valid_tracks = HashSet::<TrackSid>::new();

        for track in info.tracks {
            if let Some(publication) = self.get_track_publication(&track.sid.clone().into()) {
                publication.update_info(track.clone());
            } else {
                let publication = RemoteTrackPublication::new(track.clone(), self.sid(), None);
                self.shared
                    .add_track_publication(TrackPublication::Remote(publication.clone()));

                // This is a new track, fire publish events
                let event = TrackPublishedEvent {
                    participant: self.clone(),
                    publication: publication.clone(),
                };

                if let Some(cb) = self
                    .shared
                    .internal_events
                    .on_track_published
                    .lock()
                    .as_mut()
                {
                    cb(event.clone()).await;
                }

                if let Some(cb) = self.shared.events.on_track_published.lock().as_mut() {
                    cb(event).await;
                }
            }

            valid_tracks.insert(track.sid.into());
        }
    }
}

impl ParticipantInternalTrait for RemoteParticipant {
    fn internal_events(&self) -> Arc<ParticipantEvents> {
        self.shared.internal_events.clone()
    }
}

impl_participant_trait!(RemoteParticipant);
