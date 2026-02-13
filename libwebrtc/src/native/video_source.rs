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
    native::user_timestamp::UserTimestampStore,
    video_frame::{I420Buffer, VideoBuffer, VideoFrame},
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
    user_timestamp_store: Option<UserTimestampStore>,
}

impl NativeVideoSource {
    pub fn new(resolution: VideoResolution) -> NativeVideoSource {
        let source = Self {
            sys_handle: vt_sys::ffi::new_video_track_source(&vt_sys::ffi::VideoResolution::from(
                resolution.clone(),
            )),
            inner: Arc::new(Mutex::new(VideoSourceInner {
                captured_frames: 0,
                user_timestamp_store: None,
            })),
        };

        livekit_runtime::spawn({
            let source = source.clone();
            let i420 = I420Buffer::new(resolution.width, resolution.height);
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
                    builder.pin_mut().set_video_frame_buffer(i420.as_ref().sys_handle());

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

        let capture_ts = if frame.timestamp_us == 0 {
            // If the timestamp is set to 0, default to now
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            now.as_micros() as i64
        } else {
            frame.timestamp_us
        };
        builder.pin_mut().set_timestamp_us(capture_ts);

        // If a user timestamp is provided and a store is available, record
        // the mapping so the UserTimestampTransformer can embed it into the
        // encoded RTP frame.
        if let Some(user_ts) = frame.user_timestamp_us {
            if let Some(store) = &inner.user_timestamp_store {
                store.store(capture_ts, user_ts);
            }
        }

        self.sys_handle.on_captured_frame(&builder.pin_mut().build());
    }

    /// Set the user timestamp store used by this source.
    ///
    /// When set, any frame captured with a `user_timestamp_us` value will
    /// automatically have its timestamp pushed into the store so the
    /// `UserTimestampTransformer` can embed it into the encoded frame.
    pub fn set_user_timestamp_store(&self, store: UserTimestampStore) {
        self.inner.lock().user_timestamp_store = Some(store);
    }

    pub fn video_resolution(&self) -> VideoResolution {
        self.sys_handle.video_resolution().into()
    }
}
