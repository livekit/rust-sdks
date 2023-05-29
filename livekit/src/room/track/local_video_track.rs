use super::TrackInner;
use crate::rtc_engine::lk_runtime::LkRuntime;
use crate::{options::VideoCaptureOptions, prelude::*};
use livekit_protocol as proto;
use livekit_webrtc as rtc;
use livekit_webrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
use parking_lot::Mutex;
use rtc::video_source::native::NativeVideoSource;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug)]
struct LocalVideoTrackInner {
    track_inner: TrackInner,
    capture_options: Mutex<VideoCaptureOptions>,
}

#[derive(Clone, Debug)]
pub struct LocalVideoTrack {
    inner: Arc<LocalVideoTrackInner>,
}

impl LocalVideoTrack {
    pub fn new(
        name: String,
        rtc_track: rtc::media_stream::RtcVideoTrack,
        capture_options: VideoCaptureOptions,
    ) -> Self {
        Self {
            inner: Arc::new(LocalVideoTrackInner {
                track_inner: TrackInner::new(
                    "unknown".to_string().into(), // sid
                    name,
                    TrackKind::Video,
                    rtc::media_stream::MediaStreamTrack::Video(rtc_track),
                    None
                ),
                capture_options: Mutex::new(capture_options),
            }),
        }
    }

    #[inline]
    pub fn capture_options(&self) -> VideoCaptureOptions {
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
    pub fn rtc_track(&self) -> rtc::media_stream::RtcVideoTrack {
        if let rtc::media_stream::MediaStreamTrack::Video(video) =
            self.inner.track_inner.rtc_track()
        {
            video
        } else {
            unreachable!()
        }
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
    pub(crate) fn transceiver(&self) -> Option<rtc::rtp_transceiver::RtpTransceiver> {
        self.inner.track_inner.transceiver()
    }

    #[inline]
    pub(crate) fn update_transceiver(
        &self,
        transceiver: Option<rtc::rtp_transceiver::RtpTransceiver>,
    ) {
        self.inner.track_inner.update_transceiver(transceiver)
    }

    #[inline]
    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        self.inner.track_inner.update_info(info)
    }
}

impl LocalVideoTrack {
    pub fn create_video_track(
        name: &str,
        options: VideoCaptureOptions,
        source: NativeVideoSource,
    ) -> LocalVideoTrack {
        let rtc_track = LkRuntime::instance()
            .pc_factory
            .create_video_track(&rtc::native::create_random_uuid(), source);

        Self::new(name.to_string(), rtc_track, options)
    }
}
