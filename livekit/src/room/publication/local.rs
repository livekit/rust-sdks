use super::TrackPublicationInner;
use std::sync::Arc;
use crate::track::LocalTrack;

#[derive(Clone, Debug)]
pub struct LocalTrackPublication {
    inner: Arc<TrackPublicationInner>,
}

impl LocalTrackPublication {
    pub fn new(info: proto::TrackInfo, track: LocalTrack) -> Self {
        Self {
            inner: Arc::new(TrackPublicationInner::new(info, Some(track.into()))),
        }
    }

    #[inline]
    pub fn sid(&self) -> TrackSid {
        self.inner.sid()
    }

    #[inline]
    pub fn name(&self) -> String {
        self.inner.name()
    }

    #[inline]
    pub fn kind(&self) -> TrackKind {
        self.inner.kind()
    }

    #[inline]
    pub fn source(&self) -> TrackSource {
        self.inner.source()
    }

    #[inline]
    pub fn simulcasted(&self) -> bool {
        self.inner.simulcasted()
    }

    #[inline]
    pub fn dimension(&self) -> TrackDimension {
        self.inner.dimension()
    }

    #[inline]
    pub fn track(&self) -> LocalTrack {
        self.inner.track.lock().clone().unwrap().try_into().unwrap()
    }

    #[inline]
    pub fn mime_type(&self) -> String {
        self.inner.mime_type()
    }

    #[inline]
    pub fn muted(&self) -> bool {
        self.inner.muted()
    }

    #[inline]
    pub(crate) fn update_track(&self, track: Option<Track>) {
        self.inner.update_track(track);
    }

    #[inline]
    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        self.inner.update_info(info);
    }
}
