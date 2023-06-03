use super::TrackInner;
use crate::options::AudioCaptureOptions;
use crate::prelude::*;
use crate::rtc_engine::lk_runtime::LkRuntime;
use crate::webrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
use livekit_protocol as proto;
use livekit_webrtc::prelude::*;
use parking_lot::Mutex;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct LocalAudioTrackInner {
    track_inner: TrackInner,
    capture_options: Mutex<AudioCaptureOptions>,
}

#[derive(Clone)]
pub struct LocalAudioTrack {
    inner: Arc<LocalAudioTrackInner>,
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
    pub(crate) fn new(
        name: String,
        rtc_track: RtcAudioTrack,
        capture_options: AudioCaptureOptions,
    ) -> Self {
        Self {
            inner: Arc::new(LocalAudioTrackInner {
                track_inner: TrackInner::new(
                    "unknown".to_string().into(), // sid
                    name,
                    TrackKind::Audio,
                    MediaStreamTrack::Audio(rtc_track),
                ),
                capture_options: Mutex::new(capture_options),
            }),
        }
    }

    #[inline]
    pub fn capture_options(&self) -> AudioCaptureOptions {
        self.inner.capture_options.lock().clone()
    }

    #[inline]
    pub fn sid(&self) -> TrackSid {
        self.inner.track_inner.sid()
    }

    #[inline]
    pub fn name(&self) -> String {
        self.inner.track_inner.name()
    }

    #[inline]
    pub fn kind(&self) -> TrackKind {
        self.inner.track_inner.kind()
    }

    #[inline]
    pub fn source(&self) -> TrackSource {
        self.inner.track_inner.source()
    }

    #[inline]
    pub fn stream_state(&self) -> StreamState {
        self.inner.track_inner.stream_state()
    }

    #[inline]
    pub fn start(&self) {
        self.inner.track_inner.start()
    }

    #[inline]
    pub fn stop(&self) {
        self.inner.track_inner.stop()
    }

    #[inline]
    pub fn is_muted(&self) -> bool {
        self.inner.track_inner.is_muted()
    }

    #[inline]
    pub fn set_muted(&self, muted: bool) {
        self.inner.track_inner.set_muted(muted)
    }

    #[inline]
    pub fn rtc_track(&self) -> RtcAudioTrack {
        if let MediaStreamTrack::Audio(audio) = self.inner.track_inner.rtc_track() {
            return audio;
        }
        unreachable!()
    }

    #[inline]
    pub fn register_observer(&self) -> mpsc::UnboundedReceiver<TrackEvent> {
        self.inner.track_inner.register_observer()
    }

    #[inline]
    pub fn is_remote(&self) -> bool {
        false
    }

    #[inline]
    pub(crate) fn transceiver(&self) -> Option<RtpTransceiver> {
        self.inner.track_inner.transceiver()
    }

    #[inline]
    pub(crate) fn update_transceiver(&self, transceiver: Option<RtpTransceiver>) {
        self.inner.track_inner.update_transceiver(transceiver)
    }

    #[inline]
    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        self.inner.track_inner.update_info(info)
    }
}

impl LocalAudioTrack {
    pub fn create_audio_track(
        name: &str,
        options: AudioCaptureOptions,
        source: livekit_webrtc::audio_source::native::NativeAudioSource,
    ) -> LocalAudioTrack {
        let rtc_track = LkRuntime::instance()
            .pc_factory()
            .create_audio_track(&livekit_webrtc::native::create_random_uuid(), source);

        Self::new(name.to_string(), rtc_track, options)
    }
}
