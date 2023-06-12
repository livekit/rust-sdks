use super::TrackInner;
use crate::prelude::*;
use crate::rtc_engine::lk_runtime::LkRuntime;
use core::panic;
use livekit_protocol as proto;
use livekit_webrtc::prelude::*;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct LocalAudioTrack {
    inner: Arc<TrackInner>,
    source: RtcAudioSource,
}

impl Debug for LocalAudioTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalAudioTrack")
            .field("sid", &self.sid())
            .field("name", &self.name())
            .field("source", &self.source())
            .finish()
    }
}

impl LocalAudioTrack {
    pub(crate) fn new(name: String, rtc_track: RtcAudioTrack, source: RtcAudioSource) -> Self {
        Self {
            inner: Arc::new(TrackInner::new(
                "unknown".to_string().into(), // sid
                name,
                TrackKind::Audio,
                MediaStreamTrack::Audio(rtc_track),
            )),
            source,
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
    pub fn start(&self) {
        self.inner.start()
    }

    #[inline]
    pub fn stop(&self) {
        self.inner.stop()
    }

    #[inline]
    pub fn is_muted(&self) -> bool {
        self.inner.is_muted()
    }

    #[inline]
    pub fn set_muted(&self, muted: bool) {
        self.inner.set_muted(muted)
    }

    #[inline]
    pub fn rtc_track(&self) -> RtcAudioTrack {
        if let MediaStreamTrack::Audio(audio) = self.inner.rtc_track() {
            return audio;
        }
        unreachable!()
    }

    #[inline]
    pub fn rtc_source(&self) -> RtcAudioSource {
        self.source.clone()
    }

    #[inline]
    pub fn register_observer(&self) -> mpsc::UnboundedReceiver<TrackEvent> {
        self.inner.register_observer()
    }

    #[inline]
    pub fn is_remote(&self) -> bool {
        false
    }

    #[inline]
    pub(crate) fn transceiver(&self) -> Option<RtpTransceiver> {
        self.inner.transceiver()
    }

    #[inline]
    pub(crate) fn update_transceiver(&self, transceiver: Option<RtpTransceiver>) {
        self.inner.update_transceiver(transceiver)
    }

    #[inline]
    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        self.inner.update_info(info)
    }
}

impl LocalAudioTrack {
    pub fn create_audio_track(name: &str, source: RtcAudioSource) -> LocalAudioTrack {
        let rtc_track = match source.clone() {
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Native(native_source) => {
                use livekit_webrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
                LkRuntime::instance().pc_factory().create_audio_track(
                    &livekit_webrtc::native::create_random_uuid(),
                    native_source,
                )
            }
            _ => panic!("unsupported audio source"),
        };
        Self::new(name.to_string(), rtc_track, source)
    }
}
