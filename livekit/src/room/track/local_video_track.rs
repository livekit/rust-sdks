use super::TrackInner;
use crate::options::video_quality_for_rid;
use crate::proto;
use crate::{options::VideoCaptureOptions, prelude::*};
use tokio::sync::mpsc;
use livekit_webrtc as rtc;
use parking_lot::Mutex;
use rtc::rtp_parameters::RtpEncodingParameters;
use std::sync::Arc;

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
        sid: TrackSid,
        name: String,
        rtc_track: rtc::media_stream::VideoTrack,
        capture_options: VideoCaptureOptions,
    ) -> Self {
        Self {
            inner: Arc::new(LocalVideoTrackInner {
                track_inner: TrackInner::new(
                    sid,
                    name,
                    TrackKind::Video,
                    rtc::media_stream::MediaStreamTrack::Video(rtc_track),
                ),
                capture_options: Mutex::new(capture_options),
            }),
        }
    }

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
    pub fn muted(&self) -> bool {
        self.inner.track_inner.muted()
    }

    #[inline]
    pub fn set_muted(&self, muted: bool) {
        self.inner.track_inner.set_muted(muted)
    }

    #[inline]
    pub fn rtc_track(&self) -> rtc::media_stream::VideoTrack {
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
    pub(crate) fn set_source(&self, source: TrackSource) {
        self.inner.track_inner.set_source(source)
    }
}

pub fn video_layers_from_encodings(
    width: u32,
    height: u32,
    encodings: &[RtpEncodingParameters],
) -> Vec<proto::VideoLayer> {
    if encodings.is_empty() {
        return vec![proto::VideoLayer {
            quality: proto::VideoQuality::High as i32,
            width,
            height,
            bitrate: 0,
            ssrc: 0,
        }];
    }

    let mut layers = Vec::with_capacity(encodings.len());
    for encoding in encodings {
        let scale = encoding.scale_resolution_down_by.unwrap_or(1.0);
        let quality = video_quality_for_rid(&encoding.rid).unwrap_or(proto::VideoQuality::High);

        layers.push(proto::VideoLayer {
            quality: quality as i32,
            width: (width as f64 / scale) as u32,
            height: (height as f64 / scale) as u32,
            bitrate: encoding.max_bitrate.unwrap_or(0) as u32,
            ssrc: 0,
        });
    }

    layers
}
