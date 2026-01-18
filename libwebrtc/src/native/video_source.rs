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
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use cxx::SharedPtr;
use livekit_runtime::interval;
use parking_lot::Mutex;
use webrtc_sys::{video_frame as vf_sys, video_frame::ffi::VideoRotation, video_track as vt_sys};

use crate::{
    video_frame::{I420Buffer, NV12Buffer, VideoBuffer, VideoFrame},
    video_source::VideoResolution,
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
    inner: Arc<Mutex<VideoSourceInner>>,
}

struct VideoSourceInner {
    captured_frames: usize,
}

impl NativeVideoSource {
    pub fn new(resolution: VideoResolution) -> NativeVideoSource {
        let source = Self {
            sys_handle: vt_sys::ffi::new_video_track_source(&vt_sys::ffi::VideoResolution::from(
                resolution.clone(),
            )),
            inner: Arc::new(Mutex::new(VideoSourceInner { captured_frames: 0 })),
        };

        livekit_runtime::spawn({
            let source = source.clone();
            // Pre-roll: WebRTC expects at least one frame; we emit black frames
            // until the first real capture arrives. When capture is NV12-gated
            // (Jetson path), emit NV12 here too to avoid encoder format mismatch.
            let use_nv12 = std::env::var("LK_CAPTURE_NV12")
                .ok()
                .map_or(cfg!(all(target_os = "linux", target_arch = "aarch64")), |v| v == "1");
            let i420 = I420Buffer::new(resolution.width, resolution.height);
            let nv12 = NV12Buffer::new(resolution.width, resolution.height);
            async move {
                let mut interval = interval(Duration::from_millis(100)); // 10 fps

                loop {
                    interval.tick().await;

                    let inner = source.inner.lock();
                    if inner.captured_frames > 0 {
                        break;
                    }

                    let mut builder = vf_sys::ffi::new_video_frame_builder();
                    builder.pin_mut().set_rotation(VideoRotation::VideoRotation0);
                    if use_nv12 {
                        builder
                            .pin_mut()
                            .set_video_frame_buffer(nv12.as_ref().sys_handle());
                    } else {
                        builder
                            .pin_mut()
                            .set_video_frame_buffer(i420.as_ref().sys_handle());
                    }

                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                    builder.pin_mut().set_timestamp_us(now.as_micros() as i64);

                    source.sys_handle.on_captured_frame(&builder.pin_mut().build());
                }
            }
        });

        source
    }

    pub fn sys_handle(&self) -> SharedPtr<vt_sys::ffi::VideoTrackSource> {
        self.sys_handle.clone()
    }

    pub fn capture_frame<T: AsRef<dyn VideoBuffer>>(&self, frame: &VideoFrame<T>) {
        let mut inner = self.inner.lock();
        inner.captured_frames += 1;

        let mut builder = vf_sys::ffi::new_video_frame_builder();
        builder.pin_mut().set_rotation(frame.rotation.into());
        builder.pin_mut().set_video_frame_buffer(frame.buffer.as_ref().sys_handle());

        if frame.timestamp_us == 0 {
            // If the timestamp is set to 0, default to now
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            builder.pin_mut().set_timestamp_us(now.as_micros() as i64);
        } else {
            builder.pin_mut().set_timestamp_us(frame.timestamp_us);
        }

        self.sys_handle.on_captured_frame(&builder.pin_mut().build());
    }

    pub fn video_resolution(&self) -> VideoResolution {
        self.sys_handle.video_resolution().into()
    }
}
