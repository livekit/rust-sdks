#[derive(Clone, Debug)]
pub struct RemoteTrackPublication {
    inner: Arc<TrackPublicationInner>,
}

impl RemoteTrackPublication {
    pub fn new(info: proto::TrackInfo, track: Option<Track>) -> Self {
        Self {
            inner: Arc::new(TrackPublicationInner::new(info, track)),
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
    pub fn track(&self) -> Option<RemoteTrack> {
        self.inner
            .track
            .lock()
            .clone()
            .map(|track| track.try_into().unwrap())
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
