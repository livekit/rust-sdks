// Copyright 2025 LiveKit, Inc.
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
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use cxx::SharedPtr;
use livekit_runtime::interval;
use webrtc_sys::{video_frame as vf_sys, video_frame::ffi::VideoRotation, video_track as vt_sys};

use crate::{
    native::packet_trailer::PacketTrailerHandler,
    video_frame::{EncodedVideoFrame, I420Buffer, VideoBuffer, VideoFrame},
    video_source::{EncodedRateControl, VideoResolution},
};

impl From<vt_sys::ffi::VideoResolution> for VideoResolution {
    fn from(res: vt_sys::ffi::VideoResolution) -> Self {
        Self { width: res.width, height: res.height }
    }
}

impl From<VideoResolution> for vt_sys::ffi::VideoResolution {
    fn from(res: VideoResolution) -> Self {
        Self { width: res.width, height: res.height }
    }
}

#[derive(Clone)]
pub struct NativeVideoSource {
    sys_handle: SharedPtr<vt_sys::ffi::VideoTrackSource>,
    captured_frames: Arc<AtomicUsize>,
}

impl NativeVideoSource {
    pub fn new(resolution: VideoResolution, is_screencast: bool) -> NativeVideoSource {
        Self::new_inner(resolution, is_screencast, true)
    }

    /// Creates a source for pre-encoded access units.
    ///
    /// Unlike [`NativeVideoSource::new`], no raw black-frame keepalive is
    /// injected before the first capture: raw frames would start a real
    /// encoder on a sender meant for the pass-through encoder and corrupt
    /// the encoded stream.
    pub fn new_encoded(resolution: VideoResolution) -> NativeVideoSource {
        Self::new_inner(resolution, false, false)
    }

    fn new_inner(
        resolution: VideoResolution,
        is_screencast: bool,
        raw_keepalive: bool,
    ) -> NativeVideoSource {
        let source = Self {
            sys_handle: vt_sys::ffi::new_video_track_source(
                &vt_sys::ffi::VideoResolution::from(resolution.clone()),
                is_screencast,
            ),
            captured_frames: Arc::new(AtomicUsize::new(0)),
        };

        if raw_keepalive {
            livekit_runtime::spawn({
                let source = source.clone();
                // This buffer reaches the encoder without any plane ever being
                // written, so it must be black-initialized: `I420Buffer::new`
                // leaves the pixel data uninitialized and would leak recycled
                // heap memory to subscribers in the first keyframes.
                let i420 = I420Buffer::new_black(resolution.width, resolution.height);
                async move {
                    let mut interval = interval(Duration::from_millis(100)); // 10 fps

                    loop {
                        interval.tick().await;

                        if source.captured_frames.load(Ordering::Relaxed) > 0 {
                            break;
                        }

                        let mut builder = vf_sys::ffi::new_video_frame_builder();
                        builder.pin_mut().set_rotation(VideoRotation::VideoRotation0);
                        builder.pin_mut().set_video_frame_buffer(i420.as_ref().sys_handle());

                        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                        builder.pin_mut().set_timestamp_us(now.as_micros() as i64);

                        source.sys_handle.on_captured_frame(
                            &builder.pin_mut().build(),
                            &vt_sys::ffi::FrameMetadata {
                                has_packet_trailer: false,
                                user_timestamp: 0,
                                frame_id: 0,
                                user_data: Vec::new(),
                            },
                        );
                    }
                }
            });
        }

        source
    }

    pub fn sys_handle(&self) -> SharedPtr<vt_sys::ffi::VideoTrackSource> {
        self.sys_handle.clone()
    }

    /// Returns `false` if the frame was dropped by the adapter (e.g. due to
    /// resolution/frame-rate adaptation) instead of being forwarded.
    pub fn capture_frame<T: AsRef<dyn VideoBuffer>>(&self, frame: &VideoFrame<T>) -> bool {
        let mut builder = vf_sys::ffi::new_video_frame_builder();
        builder.pin_mut().set_rotation(frame.rotation.into());
        builder.pin_mut().set_video_frame_buffer(frame.buffer.as_ref().sys_handle());

        let capture_ts = if frame.timestamp_us == 0 {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            now.as_micros() as i64
        } else {
            frame.timestamp_us
        };
        builder.pin_mut().set_timestamp_us(capture_ts);

        let (has_trailer, user_ts, fid, user_data) = match &frame.frame_metadata {
            Some(meta) => (
                true,
                meta.user_timestamp.unwrap_or(0),
                meta.frame_id.unwrap_or(0),
                meta.user_data.clone().unwrap_or_default(),
            ),
            None => (false, 0, 0, Vec::new()),
        };

        self.captured_frames.fetch_add(1, Ordering::Relaxed);

        self.sys_handle.on_captured_frame(
            &builder.pin_mut().build(),
            &vt_sys::ffi::FrameMetadata {
                has_packet_trailer: has_trailer,
                user_timestamp: user_ts,
                frame_id: fid,
                user_data,
            },
        )
    }

    pub fn capture_encoded_frame(&self, frame: &EncodedVideoFrame<'_>) -> bool {
        let (has_trailer, user_ts, fid, user_data) = match &frame.frame_metadata {
            Some(meta) => (
                true,
                meta.user_timestamp.unwrap_or(0),
                meta.frame_id.unwrap_or(0),
                meta.user_data.clone().unwrap_or_default(),
            ),
            None => (false, 0, 0, Vec::new()),
        };

        let capture_ts = if frame.timestamp_us == 0 {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            now.as_micros() as i64
        } else {
            frame.timestamp_us
        };

        self.captured_frames.fetch_add(1, Ordering::Relaxed);
        self.sys_handle.capture_encoded_frame(
            frame.resolution.width as i32,
            frame.resolution.height as i32,
            &vt_sys::ffi::EncodedVideoFrameData {
                codec: frame.codec.into(),
                frame_type: frame.frame_type.into(),
                timestamp_us: capture_ts,
            },
            frame.payload,
            &vt_sys::ffi::FrameMetadata {
                has_packet_trailer: has_trailer,
                user_timestamp: user_ts,
                frame_id: fid,
                user_data,
            },
        )
    }

    /// Returns and clears the pending keyframe request raised by the
    /// pass-through encoder (PLI/FIR or reconfiguration). Poll from the
    /// capture loop and forward the request to the upstream encoder.
    pub fn take_keyframe_request(&self) -> bool {
        self.sys_handle.take_keyframe_request()
    }

    /// Returns and clears the pending rate-control target raised by the
    /// pass-through encoder.
    pub fn take_rate_control_request(&self) -> Option<EncodedRateControl> {
        let request = self.sys_handle.take_rate_control_request();
        request.has_request.then_some(EncodedRateControl {
            target_bitrate_bps: request.target_bitrate_bps,
            framerate_fps: request.framerate_fps,
        })
    }

    /// Set the packet trailer handler used by this source.
    ///
    /// When set, any frame captured with a `user_timestamp` value will
    /// automatically have its timestamp stored in the handler so the
    /// `PacketTrailerTransformer` can embed it into the encoded frame.
    /// The handler is set on the C++ VideoTrackSource so it has access to
    /// the TimestampAligner-adjusted capture timestamp for correct keying.
    pub fn set_packet_trailer_handler(&self, handler: PacketTrailerHandler) {
        self.sys_handle.set_packet_trailer_handler(handler.sys_handle());
    }

    pub fn video_resolution(&self) -> VideoResolution {
        self.sys_handle.video_resolution().into()
    }
}
