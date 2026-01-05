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

use crate::sys;
use crate::enum_dispatch;
use crate::video_frame::{VideoBuffer, VideoFrame};
use crate::video_frame_buffer::{new_video_frame_buffer, VideoFrameBuffer};
use crate::video_frame_builder::new_video_frame_builder;
use crate::video_source::native::NativeVideoSource;

#[derive(Debug, Clone)]
pub struct VideoResolution {
    pub width: u32,
    pub height: u32,
}

impl Default for VideoResolution {
    // Default to 720p
    fn default() -> Self {
        VideoResolution { width: 1280, height: 720 }
    }
}

impl From<VideoResolution> for sys::lkVideoResolution {
    fn from(options: VideoResolution) -> Self {
        sys::lkVideoResolution { width: options.width, height: options.height }
    }
}

impl From<sys::lkVideoResolution> for VideoResolution {
    fn from(options: sys::lkVideoResolution) -> Self {
        VideoResolution { width: options.width, height: options.height }
    }
}

pub struct VideoTrackSourceConstraints {
    pub min_fps: f64,
    pub max_fps: f64,
}

pub struct VideoTrackSource {
    pub ffi: sys::RefCounted<sys::lkVideoTrackSource>,
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum RtcVideoSource {
    // TODO(theomonnom): Web video sources (eq. to tracks on browsers?)
    #[cfg(not(target_arch = "wasm32"))]
    Native(native::NativeVideoSource),
}

// TODO(theomonnom): Support enum dispatch with conditional compilation?
impl RtcVideoSource {
    enum_dispatch!(
        [Native];
        pub fn video_resolution(self: &Self) -> VideoResolution;
    );
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::fmt::{Debug, Formatter};

    use crate::impl_thread_safety;

    use super::*;

    #[derive(Clone)]
    pub struct NativeVideoSource {
        pub ffi: sys::RefCounted<sys::lkVideoTrackSource>,
        pub inner: Arc<Mutex<VideoSourceInner>>,
    }
    #[derive(Clone)]
    pub struct VideoSourceInner {
        pub captured_frames: usize,
    }

    impl Debug for VideoSourceInner {
        fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
            f.debug_struct("VideoSourceInner")
                .field("captured_frames", &self.captured_frames)
                .finish()
        }
    }

    impl Debug for NativeVideoSource {
        fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
            f.debug_struct("NativeVideoSource").finish()
        }
    }

    impl Default for NativeVideoSource {
        fn default() -> Self {
            Self::new(VideoResolution::default())
        }
    }

    pub trait VideoSink: Send {
        fn on_frame(&self, frame: VideoFrame);
        fn on_discarded_frame(&self);
        fn on_constraints_changed(&self, constraints: VideoTrackSourceConstraints);
    }

    pub struct VideoSinkWrapper {
        observer: Arc<dyn VideoSink>,
    }

    impl VideoSinkWrapper {
        pub fn new(observer: Arc<dyn VideoSink>) -> Self {
            Self { observer }
        }
        pub fn on_frame<T: AsRef<dyn VideoBuffer>>(&self, frame: VideoFrame) {
            self.observer.on_frame(frame);
        }

        pub fn on_discarded_frame(&self) {
            self.observer.on_discarded_frame();
        }

        pub fn on_constraints_changed(&self, constraints: VideoTrackSourceConstraints) {
            self.observer.on_constraints_changed(constraints);
        }
    }

    pub static VIDEO_SINK_OBSERVER: sys::lkVideoSinkCallabacks = sys::lkVideoSinkCallabacks {
        onFrame: Some(NativeVideoSink::lk_on_frame),
        onDiscardedFrame: Some(NativeVideoSink::lk_on_discared_frame),
        onConstraintsChanged: Some(NativeVideoSink::lk_on_constraints_changed),
    };

    pub struct NativeVideoSink {
        pub ffi: sys::RefCounted<sys::lkNativeVideoSink>,
        pub observer: Arc<dyn VideoSink>,
    }

    impl NativeVideoSink {
        pub fn new(video_sink_wrapper: Arc<dyn VideoSink>) -> Self {
            let video_sink_box: *mut Arc<dyn VideoSink> =
                Box::into_raw(Box::new(video_sink_wrapper.clone()));
            let ffi = unsafe {
                sys::lkCreateNativeVideoSink(
                    &VIDEO_SINK_OBSERVER,
                    video_sink_box as *mut ::std::os::raw::c_void,
                )
            };
            Self { ffi: unsafe { sys::RefCounted::from_raw(ffi) }, observer: video_sink_wrapper }
        }

        pub extern "C" fn lk_on_constraints_changed(
            constraints: *mut sys::lkVideoTrackSourceConstraints,
            userdata: *mut ::std::os::raw::c_void,
        ) {
            let video_sink_wrapper = unsafe { &*(userdata as *const Arc<dyn VideoSink>) };
            let constraints = VideoTrackSourceConstraints {
                min_fps: unsafe { (*constraints).minFps },
                max_fps: unsafe { (*constraints).maxFps },
            };
            video_sink_wrapper.on_constraints_changed(constraints);
        }

        pub extern "C" fn lk_on_discared_frame(userdata: *mut ::std::os::raw::c_void) {
            let video_sink_wrapper = unsafe { &*(userdata as *const Arc<dyn VideoSink>) };
            video_sink_wrapper.on_discarded_frame();
        }

        pub extern "C" fn lk_on_frame(
            lkframe: *const sys::lkVideoFrame,
            userdata: *mut ::std::os::raw::c_void,
        ) {
            let video_sink_wrapper = unsafe { &*(userdata as *const Arc<dyn VideoSink>) };
            let rotation = unsafe { sys::lkVideoFrameGetRotation(lkframe) };
            let timestamp_us = unsafe { sys::lkVideoFrameGetTimestampUs(lkframe) };
            let buffer = unsafe { sys::lkVideoFrameGetBuffer(lkframe) };
            let video_frame_buffer =
                VideoFrameBuffer { ffi: unsafe { sys::RefCounted::from_raw(buffer) } };
            video_sink_wrapper.on_frame(VideoFrame {
                rotation: rotation.into(),
                timestamp_us: timestamp_us,
                buffer: new_video_frame_buffer(video_frame_buffer),
            })
        }
    }

    impl Debug for NativeVideoSink {
        fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
            f.debug_struct("NativeVideoSink").finish()
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {}

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::interval;

use crate::video_frame::{I420Buffer, VideoRotation};
use crate::video_source::native::VideoSourceInner;
use parking_lot::Mutex;

impl NativeVideoSource {
    pub fn new(resolution: VideoResolution) -> NativeVideoSource {
        let res_copy = resolution.clone();
        let ffi = unsafe { sys::lkCreateVideoTrackSource(res_copy.into()) };
        let source = Self {
            ffi: unsafe { sys::RefCounted::from_raw(ffi) },
            inner: Arc::new(Mutex::new(VideoSourceInner { captured_frames: 0 })),
        };

        let clone_source = source.clone();
        livekit_runtime::spawn({
            let i420 = I420Buffer::new(resolution.width, resolution.height);
            async move {
                let mut interval = interval(Duration::from_millis(100)); // 10 fps

                loop {
                    interval.tick().await;

                    let inner = clone_source.inner.lock();
                    if inner.captured_frames > 0 {
                        break;
                    }

                    let mut builder = new_video_frame_builder();
                    builder.set_rotation(VideoRotation::VideoRotation0);
                    builder.set_video_frame_buffer(i420.ffi.clone());

                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                    builder.set_timestamp_us(now.as_micros() as i64);
                    let frame = builder.build();
                    unsafe {
                        sys::lkVideoTrackSourceOnCaptureFrame(
                            clone_source.ffi.as_ptr(),
                            frame.as_ptr(),
                        )
                    }
                }
            }
        });

        source
    }

    pub fn source(&self) -> VideoTrackSource {
        VideoTrackSource { ffi: self.ffi.clone() }
    }

    pub fn capture_frame(&self, frame: &VideoFrame) {
        let mut inner = self.inner.lock();
        inner.captured_frames += 1;

        let mut builder = new_video_frame_builder();
        builder.set_rotation(frame.rotation.into());
        builder.set_video_frame_buffer(frame.buffer.ffi().clone());

        if frame.timestamp_us == 0 {
            // If the timestamp is set to 0, default to now
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            builder.set_timestamp_us(now.as_micros() as i64);
        } else {
            builder.set_timestamp_us(frame.timestamp_us);
        }

        let frame = builder.build();
        unsafe { sys::lkVideoTrackSourceOnCaptureFrame(self.ffi.as_ptr(), frame.as_ptr()) }
    }

    pub fn video_resolution(&self) -> VideoResolution {
        unsafe { sys::lkVideoTrackSourceGetResolution(self.ffi.as_ptr()).into() }
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn create_video_native_sink() {}
}
