use super::TrackInner;
use crate::prelude::*;
use livekit_protocol as proto;
use livekit_webrtc::prelude::*;
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Clone)]
pub struct RemoteAudioTrack {
    pub(crate) inner: Arc<TrackInner>,
}

impl Debug for RemoteAudioTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteAudioTrack")
            .field("sid", &self.sid())
            .field("name", &self.name())
            .field("source", &self.source())
            .finish()
    }
}

impl RemoteAudioTrack {
    pub(crate) fn new(sid: TrackSid, name: String, rtc_track: RtcAudioTrack) -> Self {
        Self {
            inner: Arc::new(TrackInner::new(
                sid,
                name,
                TrackKind::Audio,
                MediaStreamTrack::Audio(rtc_track),
            )),
        }
    }

    pub fn sid(&self) -> TrackSid {
        self.inner.info.read().sid
    }

    pub fn name(&self) -> String {
        self.inner.info.read().name.clone()
    }

    pub fn kind(&self) -> TrackKind {
        self.inner.info.read().kind
    }

    pub fn source(&self) -> TrackSource {
        self.inner.info.read().source
    }

    pub fn stream_state(&self) -> StreamState {
        self.inner.info.read().stream_state
    }

    pub fn enable(&self) {
        self.inner.rtc_track.set_enabled(true);
    }

    pub fn disable(&self) {
        self.inner.rtc_track.set_enabled(false);
    }

    pub fn is_muted(&self) -> bool {
        self.inner.info.read().muted
    }

    pub fn rtc_track(&self) -> RtcAudioTrack {
        if let MediaStreamTrack::Audio(audio) = self.inner.rtc_track {
            return audio;
        }
        unreachable!();
    }

    pub fn is_remote(&self) -> bool {
        true
    }

    pub fn on_muted(&self, f: impl Fn()) {
        self.inner.events.write().muted = Some(Arc::new(f));
    }

    pub fn on_unmuted(&self, f: impl Fn()) {
        self.inner.events.write().unmuted = Some(Arc::new(f));
    }

    pub(crate) fn transceiver(&self) -> Option<RtpTransceiver> {
        self.inner.info.read().transceiver.clone()
    }

    pub(crate) fn set_transceiver(&self, transceiver: Option<RtpTransceiver>) {
        self.inner.info.write().transceiver = transceiver;
    }

    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        self.inner.update_info(info)
    }
}
