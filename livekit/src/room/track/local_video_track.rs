use super::TrackInner;
use crate::prelude::*;
use futures::channel::mpsc;
use livekit_webrtc as rtc;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct LocalVideoTrack {
    pub(crate) inner: Arc<TrackInner>,
}

impl LocalVideoTrack {
    pub(crate) fn new(
        sid: TrackSid,
        name: String,
        rtc_track: rtc::media_stream::VideoTrack,
    ) -> Self {
        Self {
            inner: Arc::new(TrackInner::new(
                sid,
                name,
                TrackKind::Video,
                rtc::media_stream::MediaStreamTrack::Video(rtc_track),
            )),
        }
    }

    pub fn sid(&self) -> TrackSid {
        self.inner.sid()
    }

    pub fn name(&self) -> String {
        self.inner.name()
    }

    pub fn kind(&self) -> TrackKind {
        self.inner.kind()
    }

    pub fn source(&self) -> TrackSource {
        self.inner.source()
    }

    pub fn stream_state(&self) -> StreamState {
        self.inner.stream_state()
    }

    pub fn start(&self) {
        self.inner.start()
    }

    pub fn stop(&self) {
        self.inner.stop()
    }

    pub fn muted(&self) -> bool {
        self.inner.muted()
    }

    pub fn set_muted(&self, muted: bool) {
        self.inner.set_muted(muted)
    }

    pub fn rtc_track(&self) -> rtc::media_stream::VideoTrack {
        let rtc::media_stream::MediaStreamTrack::Video(video) = self.inner.rtc_track();
        video
    }

    pub fn register_observer(&self) -> mpsc::UnboundedReceiver<TrackEvent> {
        self.inner.register_observer()
    }

    pub(crate) fn set_source(&self, source: TrackSource) {
        self.inner.set_source(source)
    }
}
