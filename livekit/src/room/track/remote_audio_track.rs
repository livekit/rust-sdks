use super::remote_track;
use super::TrackInner;
use crate::prelude::*;
use livekit_protocol as proto;
use livekit_webrtc::prelude::*;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::mpsc;

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
    pub fn stream_state(&self) -> StreamState {
        self.inner.stream_state()
    }

    #[inline]
    pub fn enable(&self) {
        self.inner.enable()
    }

    #[inline]
    pub fn disable(&self) {
        self.inner.disable()
    }

    #[inline]
    pub fn is_muted(&self) -> bool {
        self.inner.is_muted()
    }

    #[inline]
    pub fn rtc_track(&self) -> RtcAudioTrack {
        if let MediaStreamTrack::Audio(audio) = self.inner.rtc_track() {
            return audio;
        }
        unreachable!()
    }

    #[inline]
    pub fn register_observer(&self) -> mpsc::UnboundedReceiver<TrackEvent> {
        self.inner.register_observer()
    }

    #[inline]
    pub fn is_remote(&self) -> bool {
        true
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn transceiver(&self) -> Option<RtpTransceiver> {
        self.inner.transceiver()
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn update_transceiver(&self, transceiver: Option<RtpTransceiver>) {
        self.inner.update_transceiver(transceiver)
    }

    #[inline]
    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        remote_track::update_info(&self.inner, info);
    }
}
