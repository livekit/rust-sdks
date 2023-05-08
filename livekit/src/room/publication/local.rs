use super::TrackPublicationInner;
use crate::id::TrackSid;
use crate::options::TrackPublishOptions;
use crate::track::{LocalTrack, Track, TrackDimension, TrackKind, TrackSource};
use livekit_protocol as proto;
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Debug)]
struct LocalTrackPublicationInner {
    publication_inner: TrackPublicationInner,
    options: Mutex<TrackPublishOptions>,
}

#[derive(Clone, Debug)]
pub struct LocalTrackPublication {
    inner: Arc<LocalTrackPublicationInner>,
}

impl LocalTrackPublication {
    pub(crate) fn new(
        info: proto::TrackInfo,
        track: LocalTrack,
        options: TrackPublishOptions,
    ) -> Self {
        Self {
            inner: Arc::new(LocalTrackPublicationInner {
                publication_inner: TrackPublicationInner::new(info, Some(track.into())),
                options: Mutex::new(options),
            }),
        }
    }

    #[inline]
    pub fn sid(&self) -> TrackSid {
        self.inner.publication_inner.sid()
    }

    #[inline]
    pub fn name(&self) -> String {
        self.inner.publication_inner.name()
    }

    #[inline]
    pub fn kind(&self) -> TrackKind {
        self.inner.publication_inner.kind()
    }

    #[inline]
    pub fn source(&self) -> TrackSource {
        self.inner.publication_inner.source()
    }

    #[inline]
    pub fn simulcasted(&self) -> bool {
        self.inner.publication_inner.simulcasted()
    }

    #[inline]
    pub fn dimension(&self) -> TrackDimension {
        self.inner.publication_inner.dimension()
    }

    #[inline]
    pub fn track(&self) -> Option<LocalTrack> {
        self.inner
            .publication_inner
            .track()
            .map(|track| track.try_into().unwrap())
    }

    #[inline]
    pub fn mime_type(&self) -> String {
        self.inner.publication_inner.mime_type()
    }

    #[inline]
    pub fn is_muted(&self) -> bool {
        self.inner.publication_inner.is_muted()
    }

    #[inline]
    pub fn is_remote(&self) -> bool {
        false
    }

    #[inline]
    pub(crate) fn update_track(&self, track: Option<Track>) {
        self.inner.publication_inner.update_track(track);
    }

    #[inline]
    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        self.inner.publication_inner.update_info(info);
    }
}
