use crate::room::id::TrackSid;
use crate::room::participant::{impl_participant_trait, ParticipantShared};
use crate::room::track::{RemoteAudioTrack, RemoteTrack, RemoteVideoTrack, TrackKind};
use crate::room::track_publication::{
    RemoteTrackPublication, TrackPublication, TrackPublicationTrait,
};
use livekit_webrtc::media_stream::MediaStreamTrack;
use std::time::Duration;
use tokio::time::{sleep, timeout};

const ADD_TRACK_TIMEOUT: Duration = Duration::from_secs(5);


// It should be fine to add event listeners in this structure
// Registering after should be ParticipantConnected is fine to avoid missing events
pub struct RemoteParticipant {
    shared: ParticipantShared,
}

impl RemoteParticipant {
    pub(super) fn new(info: ParticipantInfo) -> Self {
        Self {
            shared: ParticipantShared::new(
                info.sid.into(),
                info.identity.into(),
                info.name,
                info.metadata,
            ),
        }
    }

    pub(super) async fn add_subscribed_media_track(
        &self,
        sid: &TrackSid,
        media_track: MediaStreamTrack,
    ) {
        let wait_publication = async {
            loop {
                let publication = self.get_track_publication(sid);
                if let Some(publication) = publication {
                    return publication;
                }

                sleep(Duration::from_millis(50)).await;
            }
        };

        let res = timeout(ADD_TRACK_TIMEOUT, wait_publication).await;

        if let Ok(remote_publication) = res {
            let track = match remote_publication.kind() {
                TrackKind::Audio => {
                    let audio_track = RemoteAudioTrack::new();
                    RemoteTrack::Audio(audio_track)
                }
                TrackKind::Video => {
                    let video_track = RemoteVideoTrack::new();
                    RemoteTrack::Video(video_track)
                }
                _ => unreachable!(),
            };



            // TODO(theomonnom): call OnTrackSubscribed here

        } else {
            // TODO(theomonnom): send error
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
}

impl_participant_trait!(RemoteParticipant);
