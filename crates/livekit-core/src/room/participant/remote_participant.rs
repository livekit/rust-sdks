use crate::events::{
    TrackError, TrackPublishedEvent, TrackSubscribedEvent, TrackSubscriptionFailedEvent,
};
use crate::proto::TrackInfo;
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
use crate::room::track::{TrackKind, TrackTrait};
use crate::room::{RoomHandle, RoomInner};
use livekit_webrtc::media_stream::MediaStreamTrackHandle;
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::{debug, debug_span, error, instrument, Instrument, Level};

use super::ParticipantTrait;

const ADD_TRACK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct RemoteParticipant {
    shared: ParticipantShared,
}

impl RemoteParticipant {
    pub(crate) fn new(
        sid: ParticipantSid,
        identity: ParticipantIdentity,
        name: String,
        metadata: String,
    ) -> Self {
        Self {
            shared: ParticipantShared::new(sid, identity, name, metadata),
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

    #[instrument(level = Level::DEBUG, skip(room_handle))]
    pub(crate) async fn add_subscribed_media_track(
        self: Arc<Self>,
        room_handle: RoomHandle,
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

                    tokio::task::yield_now();
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
            self.shared
                .add_track_publication(TrackPublication::Remote(remote_publication.clone()));
            track.start();

            let event = TrackSubscribedEvent {
                room_handle,
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
                room_handle,
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
    }

    #[instrument(level = Level::DEBUG, skip(room_handle))]
    pub(crate) async fn update_tracks(
        self: Arc<Self>,
        room_handle: RoomHandle,
        tracks: Vec<TrackInfo>,
    ) {
        let mut valid_tracks = HashSet::<TrackSid>::new();

        for track in tracks {
            if let Some(publication) = self.get_track_publication(&track.sid.clone().into()) {
                publication.update_info(track.clone());
            } else {
                let publication = RemoteTrackPublication::new(track.clone(), self.sid(), None);
                self.shared
                    .add_track_publication(TrackPublication::Remote(publication.clone()));

                // This is a new track, fire publish events
                let event = TrackPublishedEvent {
                    room_handle: room_handle.clone(),
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

    fn update_info(&self, info: ParticipantInfo) {
        self.shared.update_info(info)
    }
}

impl_participant_trait!(RemoteParticipant);
