use super::TrackInner;
use crate::prelude::*;
use crate::rtc_engine::lk_runtime::LkRuntime;
use livekit_protocol as proto;
use livekit_webrtc::prelude::*;
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Clone)]
pub struct LocalVideoTrack {
    inner: Arc<TrackInner>,
    source: RtcVideoSource,
}

impl Debug for LocalVideoTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalVideoTrack")
            .field("sid", &self.sid())
            .field("name", &self.name())
            .field("source", &self.source())
            .finish()
    }
}

impl LocalVideoTrack {
    pub fn new(name: String, rtc_track: RtcVideoTrack, source: RtcVideoSource) -> Self {
        Self {
            inner: Arc::new(TrackInner::new(
                "unknown".to_string().into(), // sid
                name,
                TrackKind::Video,
                MediaStreamTrack::Video(rtc_track),
            )),
            source,
        }
    }

    pub fn create_video_track(name: &str, source: RtcVideoSource) -> LocalVideoTrack {
        let rtc_track = match source.clone() {
            #[cfg(not(target_arch = "wasm32"))]
            RtcVideoSource::Native(native_source) => {
                use livekit_webrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
                LkRuntime::instance().pc_factory().create_video_track(
                    &livekit_webrtc::native::create_random_uuid(),
                    native_source,
                )
            }
            _ => panic!("unsupported video source"),
        };

        Self::new(name.to_string(), rtc_track, source)
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

    pub fn mute(&self) {
        self.inner.set_muted(true);
    }

    pub fn unmute(&self) {
        self.inner.set_muted(false);
    }

    pub fn rtc_track(&self) -> RtcVideoTrack {
        if let MediaStreamTrack::Video(video) = self.inner.rtc_track {
            return video;
        }
        unreachable!();
    }

    pub fn is_remote(&self) -> bool {
        false
    }

    pub fn rtc_source(&self) -> RtcVideoSource {
        self.source.clone()
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
