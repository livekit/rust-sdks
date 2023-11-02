// Copyright 2023 LiveKit, Inc.
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

use crate::video_frame::{VideoBuffer, VideoFrame};
use crate::video_source::VideoResolution;
use cxx::SharedPtr;
use std::time::{SystemTime, UNIX_EPOCH};
use webrtc_sys::video_frame as vf_sys;
use webrtc_sys::video_track as vt_sys;

impl From<vt_sys::ffi::VideoResolution> for VideoResolution {
    fn from(res: vt_sys::ffi::VideoResolution) -> Self {
        Self {
            width: res.width,
            height: res.height,
        }
    }
}

impl From<VideoResolution> for vt_sys::ffi::VideoResolution {
    fn from(res: VideoResolution) -> Self {
        Self {
            width: res.width,
            height: res.height,
        }
    }
}

#[derive(Clone)]
pub struct NativeVideoSource {
    sys_handle: SharedPtr<vt_sys::ffi::VideoTrackSource>,
}

impl NativeVideoSource {
    pub fn new(resolution: VideoResolution) -> NativeVideoSource {
        Self {
            sys_handle: vt_sys::ffi::new_video_track_source(&vt_sys::ffi::VideoResolution::from(
                resolution,
            )),
        }
    }

    pub fn sys_handle(&self) -> SharedPtr<vt_sys::ffi::VideoTrackSource> {
        self.sys_handle.clone()
    }

    pub fn capture_frame<T: AsRef<dyn VideoBuffer>>(&self, frame: &VideoFrame<T>) {
        let mut builder = vf_sys::ffi::new_video_frame_builder();
        builder.pin_mut().set_rotation(frame.rotation.into());
        builder
            .pin_mut()
            .set_video_frame_buffer(frame.buffer.as_ref().sys_handle());

        if frame.timestamp_us == 0 {
            // If the timestamp is set to 0, default to now
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            builder.pin_mut().set_timestamp_us(now.as_micros() as i64);
        } else {
            builder.pin_mut().set_timestamp_us(frame.timestamp_us);
        }

        self.sys_handle
            .on_captured_frame(&builder.pin_mut().build());
    }

    pub fn video_resolution(&self) -> VideoResolution {
        self.sys_handle.video_resolution().into()
    }
}
