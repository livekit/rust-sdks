use super::TrackPublicationInner;
use crate::participant::ParticipantInternal;
use crate::prelude::*;
use livekit_protocol as proto;
use std::sync::{Arc, Weak};

#[derive(Clone)]
pub struct LocalTrackPublication {
    inner: Arc<TrackPublicationInner>,
}

impl LocalTrackPublication {
    pub(crate) fn new(
        info: proto::TrackInfo,
        participant: Weak<ParticipantInternal>,
        track: LocalTrack,
    ) -> Self {
        Self {
            inner: Arc::new(TrackPublicationInner::new(
                info,
                participant,
                Some(track.into()),
            )),
        }
    }

    pub async fn mute(&self) {}

    pub async fn unmute(&self) {}

    pub async fn pause_upstream(&self) {}

    pub async fn resume_upstream(&self) {}

    /*pub fn set_muted(&self, muted: bool) {
        if self.is_muted() == muted {
            return;
        }

        self.track().rtc_track().set_enabled(!muted);

        let participant = self.inner.publication_inner.participant().upgrade();
        if participant.is_none() {
            log::warn!("publication's participant is invalid, set_muted failed");
            return;
        }
        let participant = participant.unwrap();

        // Engine update muted

        // Participant MUTED/UNMUTED event
    }*/

    pub fn sid(&self) -> TrackSid {
        self.inner.publication_inner.sid()
    }

    pub fn name(&self) -> String {
        self.inner.publication_inner.name()
    }

    pub fn kind(&self) -> TrackKind {
        self.inner.publication_inner.kind()
    }

    pub fn source(&self) -> TrackSource {
        self.inner.publication_inner.source()
    }

    pub fn simulcasted(&self) -> bool {
        self.inner.publication_inner.simulcasted()
    }

    pub fn dimension(&self) -> TrackDimension {
        self.inner.publication_inner.dimension()
    }

    pub fn track(&self) -> LocalTrack {
        self.inner
            .publication_inner
            .track()
            .unwrap()
            .try_into()
            .unwrap()
    }

    pub fn mime_type(&self) -> String {
        self.inner.publication_inner.mime_type()
    }

    pub fn is_muted(&self) -> bool {
        self.inner.publication_inner.is_muted()
    }

    pub fn is_remote(&self) -> bool {
        false
    }

    pub(crate) fn set_track(&self, track: Option<Track>) {
        self.inner.update_track(track);
    }

    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        self.inner.update_info(info);
    }
}
