use super::TrackPublicationInner;
use crate::id::TrackSid;
use crate::options::TrackPublishOptions;
use crate::proto;
use crate::track::{LocalTrack, Track, TrackDimension, TrackKind, TrackSource};
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
    pub fn new(info: proto::TrackInfo, track: LocalTrack, options: TrackPublishOptions) -> Self {
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
    pub fn track(&self) -> LocalTrack {
        self.inner
            .publication_inner
            .track()
            .unwrap()
            .try_into()
            .unwrap()
    }

    #[inline]
    pub fn mime_type(&self) -> String {
        self.inner.publication_inner.mime_type()
    }

    #[inline]
    pub fn muted(&self) -> bool {
        self.inner.publication_inner.muted()
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
