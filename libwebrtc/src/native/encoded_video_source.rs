// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
};

use cxx::SharedPtr;
use parking_lot::Mutex;
use webrtc_sys::encoded_video_source as sys_evs;

use crate::video_source::{EncodedFrameInfo, VideoCodec, VideoResolution};

/// Observer that receives encoder-side feedback (keyframe requests, bitrate
/// updates) for a [`NativeEncodedVideoSource`].
///
/// Callbacks are invoked on internal WebRTC threads; implementers MUST be
/// cheap and non-blocking.
pub trait EncodedVideoSourceObserver: Send + Sync {
    /// Called when the receiver requests a keyframe (PLI/FIR).
    fn on_keyframe_requested(&self);

    /// Called when the WebRTC bandwidth estimator updates the target
    /// bitrate / framerate for this source.
    fn on_target_bitrate(&self, bitrate_bps: u32, framerate_fps: f64);
}

impl From<VideoCodec> for sys_evs::ffi::EncodedVideoCodecType {
    fn from(codec: VideoCodec) -> Self {
        match codec {
            VideoCodec::H264 => Self::H264,
            VideoCodec::H265 => Self::H265,
            VideoCodec::Vp8 => Self::Vp8,
            VideoCodec::Vp9 => Self::Vp9,
            VideoCodec::Av1 => Self::Av1,
        }
    }
}

impl From<sys_evs::ffi::EncodedVideoCodecType> for VideoCodec {
    fn from(codec: sys_evs::ffi::EncodedVideoCodecType) -> Self {
        match codec {
            sys_evs::ffi::EncodedVideoCodecType::H264 => Self::H264,
            sys_evs::ffi::EncodedVideoCodecType::H265 => Self::H265,
            sys_evs::ffi::EncodedVideoCodecType::Vp8 => Self::Vp8,
            sys_evs::ffi::EncodedVideoCodecType::Vp9 => Self::Vp9,
            sys_evs::ffi::EncodedVideoCodecType::Av1 => Self::Av1,
            _ => Self::H264,
        }
    }
}

struct Inner {
    resolution: Mutex<VideoResolution>,
}

/// A video source that accepts encoded compressed frames (H.264, H.265,
/// VP8, VP9, AV1) instead of raw pixels. WebRTC's encoder is bypassed for
/// tracks bound to this source — frames flow straight from `capture_frame`
/// into RTP packetization and congestion control.
///
/// A source carries a single encoded stream (one resolution, one codec). For
/// simulcast, create several sources and publish them on separate tracks.
#[derive(Clone)]
pub struct NativeEncodedVideoSource {
    sys_handle: SharedPtr<sys_evs::ffi::EncodedVideoTrackSource>,
    inner: Arc<Inner>,
}

impl Debug for NativeEncodedVideoSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeEncodedVideoSource")
            .field("source_id", &self.source_id())
            .field("codec", &self.codec())
            .finish()
    }
}

impl NativeEncodedVideoSource {
    pub fn new(codec: VideoCodec, resolution: VideoResolution) -> Self {
        let sys_handle = sys_evs::ffi::new_encoded_video_track_source(
            codec.into(),
            resolution.width,
            resolution.height,
        );
        Self { sys_handle, inner: Arc::new(Inner { resolution: Mutex::new(resolution) }) }
    }

    /// Unique non-zero id assigned to this source. Exposed for debugging /
    /// tracing; callers do not need to inspect it.
    pub fn source_id(&self) -> u16 {
        self.sys_handle.source_id()
    }

    pub fn codec(&self) -> VideoCodec {
        self.sys_handle.codec().into()
    }

    pub fn video_resolution(&self) -> VideoResolution {
        self.inner.resolution.lock().clone()
    }

    /// Push an encoded (compressed) frame to the track. Returns `true` if the frame was
    /// accepted, `false` if the internal queue was full and the frame had to
    /// be dropped.
    pub fn capture_frame(&self, data: &[u8], info: &EncodedFrameInfo) -> bool {
        {
            let mut res = self.inner.resolution.lock();
            if info.width != 0 && info.height != 0 {
                res.width = info.width;
                res.height = info.height;
            }
        }

        self.sys_handle.capture_frame(
            data,
            info.is_keyframe,
            info.has_sps_pps,
            info.width,
            info.height,
            info.capture_time_us,
        )
    }

    /// Register an observer for encoder-side feedback. The previous observer
    /// (if any) is dropped.
    pub fn set_observer(&self, observer: Arc<dyn EncodedVideoSourceObserver>) {
        let wrapper = Box::new(sys_evs::EncodedVideoSourceWrapper::new(Arc::new(ObserverBridge {
            inner: observer,
        })));
        self.sys_handle.set_observer(wrapper);
    }

    pub fn sys_handle(&self) -> SharedPtr<sys_evs::ffi::EncodedVideoTrackSource> {
        self.sys_handle.clone()
    }
}

/// Adapts a `libwebrtc`-level observer trait object to the
/// `webrtc-sys`-level observer trait expected by the cxx bridge.
struct ObserverBridge {
    inner: Arc<dyn EncodedVideoSourceObserver>,
}

impl sys_evs::EncodedVideoSourceObserver for ObserverBridge {
    fn on_keyframe_requested(&self) {
        self.inner.on_keyframe_requested();
    }

    fn on_target_bitrate(&self, bitrate_bps: u32, framerate_fps: f64) {
        self.inner.on_target_bitrate(bitrate_bps, framerate_fps);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_source_reports_codec_and_updates_resolution_from_frames() {
        let source = NativeEncodedVideoSource::new(
            VideoCodec::Av1,
            VideoResolution { width: 640, height: 360 },
        );

        assert_ne!(source.source_id(), 0);
        assert_eq!(source.codec(), VideoCodec::Av1);
        assert_eq!(source.video_resolution().width, 640);
        assert_eq!(source.video_resolution().height, 360);

        let info = EncodedFrameInfo {
            is_keyframe: true,
            width: 1280,
            height: 720,
            capture_time_us: 123_456,
            ..Default::default()
        };

        assert!(source.capture_frame(&[0x0A, 0x00], &info));
        assert_eq!(source.video_resolution().width, 1280);
        assert_eq!(source.video_resolution().height, 720);
    }

    #[test]
    fn encoded_source_prefers_buffered_keyframe_over_incoming_delta_when_full() {
        let source = NativeEncodedVideoSource::new(
            VideoCodec::H264,
            VideoResolution { width: 640, height: 360 },
        );
        let keyframe =
            EncodedFrameInfo { is_keyframe: true, width: 640, height: 360, ..Default::default() };
        let delta = EncodedFrameInfo { width: 640, height: 360, ..Default::default() };

        assert!(source.capture_frame(&[0, 0, 0, 1, 0x65], &keyframe));
        for _ in 0..7 {
            assert!(source.capture_frame(&[0, 0, 0, 1, 0x41], &delta));
        }

        assert!(!source.capture_frame(&[0, 0, 0, 1, 0x41], &delta));
    }
}
