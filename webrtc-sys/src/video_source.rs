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

use crate::{sys};

use std::ffi::c_void;

#[derive(Debug, Clone)]
pub struct VideoResolution {
  pub width : u32, pub height : u32,
}

impl Default for VideoResolution {
  // Default to 720p
  fn default()->Self {
    VideoResolution{width : 1280, height : 720}
  }
}

impl From<VideoResolution> for sys::lkVideoResolution {
  fn from(options : VideoResolution) -> Self {
    sys::lkVideoResolution {
    width:
      options.width as i32, height : options.height as i32,
    }
  }
}

impl From<sys::lkVideoResolution> for VideoResolution {
  fn from(options : sys::lkVideoResolution) -> Self {
    VideoResolution {
    width:
      options.width, height : options.height,
    }
  }
}

pub struct VideoTrackSourceConstraints {
  pub minFps : f64, pub maxFps : f64,
}
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct RtcVideoSource {
  ffi : sys::RefCounted<sys::lkVideoTrackSource>,
        inner : Arc<Mutex<VideoSourceInner>>,
}
#[cfg(not(target_arch = "wasm32"))]
pub mod native {
  use std::fmt::{Debug, Formatter};
  use std::sync::mpsc;

  use super::*;
  use crate::video_frame::{VideoBuffer, VideoFrame};

#[derive(Clone)]
  pub struct NativeVideoSource {
    pub ffi : sys::RefCounted<sys::lkVideoTrackSource>,
        inner : Arc<Mutex<VideoSourceInner>>,
  }

  struct VideoSourceInner {
    captured_frames : usize,
}

    impl Debug for NativeVideoSource {
  fn fmt(&self, f : &mut Formatter) -> std::fmt::Result {
    f.debug_struct("NativeVideoSource").finish()
  }
}

    impl Default for NativeVideoSource {
      fn default()->Self {
        Self::new (VideoResolution::default())
      }
    }

    impl NativeVideoSource {
      pub fn new (resolution : VideoResolution)->Self {
        Self {
        handle:
          NativeVideoSource::new (resolution)
        }
      }

      pub fn capture_frame<T : AsRef<dyn VideoBuffer>>(
          &self, frame : &VideoFrame<T>){self.handle.capture_frame(frame)}

      pub fn video_resolution(&self) -> VideoResolution {
        self.handle.video_resolution()
      }
    }

    pub trait VideoSink : Send {
      fn on_frame(&self, frame : &VideoFrame<T>);
      fn on_discarded_frame(&self);
      fn on_constraints_changed(&self,
                                constraints : VideoTrackSourceConstraints);
    }

    pub struct VideoSinkWrapper {
      observer : Arc<dyn VideoSink>,
    }

    impl VideoSinkWrapper {
      pub fn new (observer : Arc<dyn VideoSink>)->Self {
        Self {
          observer
        }
      }
      pub fn on_frame(&self, frame : VideoFrame) {
        self.observer.on_frame(frame);
      }

      pub fn on_discarded_frame(&self) {
        self.observer.on_discarded_frame();
      }

      pub fn on_constraints_changed(&self,
                                    constraints : VideoTrackSourceConstraints) {
        self.observer.on_constraints_changed(constraints);
      }
    }

    pub static VIDEO_SINK_OBSERVER
        : sys::lkVideoSinkCallabacks = sys::lkVideoSinkCallabacks{
          onFrame : Some(NativeVideoSink::lk_on_frame),
          onDiscardedFrame : Some(NativeVideoSink::lk_on_discared_frame),
          onConstraintsChanged :
              Some(NativeVideoSink::lk_on_constraints_changed),
        };

    pub struct NativeVideoSink {
      ffi : sys::RefCounted<sys::lkNativeVideoSink>,
            observer : Arc<VideoSinkWrapper>,
    }

    impl NativeVideoSink {
      pub fn new (video_sink_wrapper : Arc<VideoSinkWrapper>)->Self {
        let video_sink_box
            : *mut Arc<VideoSinkWrapper> =
                  Box::into_raw(Box::new (video_sink_wrapper.clone()));
        let ffi = unsafe {
          sys::lkCreateNativeVideoSink(
              &VIDEO_SINK_OBSERVER,
              video_sink_box as * mut ::std::os::raw::c_void, );
        };
        Self {
        ffi:
          sys::RefCounted::from_raw(ffi), observer : video_sink_wrapper,
        }
      }

      pub extern "C" fn lk_on_constraints_changed(
          constraints : * const sys::lkVideoTrackSourceConstraints,
          userdata : *mut c_void) {
        let video_sink_wrapper =
            unsafe{&*(userdata as* const Arc<VideoSinkWrapper>)};
        let constraints = VideoTrackSourceConstraints{
          minFps : unsafe{(*constraints).minFps},
          maxFps : unsafe{(*constraints).maxFps},
        };
        video_sink_wrapper.on_constraints_changed(constraints);
      }

      pub extern "C" fn lk_on_discared_frame(
          userdata : *mut ::std::os::raw::c_void) {
        let video_sink_wrapper =
            unsafe{&*(userdata as* const Arc<VideoSinkWrapper>)};
        video_sink_wrapper.on_discarded_frame();
      }

      pub extern "C" fn lk_on_frame(frame : * const sys::lkVideoFrame,
                                    userdata : *mut ::std::os::raw::c_void) {
        let video_sink_wrapper =
            unsafe{&*(userdata as* const Arc<VideoSinkWrapper>)};
      }
    }

    impl Debug for NativeVideoSink {
      fn fmt(&self, f : &mut Formatter) -> std::fmt::Result {
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

use parking_lot::Mutex;
use webrtc_sys::{video_frame as vf_sys, video_frame::ffi::VideoRotation,
                 video_track as vt_sys};

use crate::{
    video_frame::{I420Buffer, VideoBuffer, VideoFrame},
};

impl From<vt_sys::ffi::VideoResolution> for VideoResolution {
  fn from(res : vt_sys::ffi::VideoResolution) -> Self {
    Self {
    width:
      res.width, height : res.height
    }
  }
}

impl From<VideoResolution> for vt_sys::ffi::VideoResolution {
  fn from(res : VideoResolution) -> Self {
    Self {
    width:
      res.width, height : res.height
    }
  }
}

impl NativeVideoSource {
  pub fn new (resolution : VideoResolution)->NativeVideoSource {
    let ffi = unsafe{sys::lkCreateVideoTrackSource(resolution.into())};
    let source = Self{
      ffi : sys::RefCounted::from_raw(ffi),
      inner : Arc::new (Mutex::new (VideoSourceInner{captured_frames : 0})),
    };

    livekit_runtime::spawn({
      let source = source.clone();
      let i420 = I420Buffer::new (resolution.width, resolution.height);
      async move {
        let mut interval = interval(Duration::from_millis(100));  // 10 fps

        loop {
          interval.tick().await;

          let inner = source.inner.lock();
          if inner
            .captured_frames > 0 {
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

  pub fn sys_handle(&self)
      -> SharedPtr<vt_sys::ffi::VideoTrackSource>{self.sys_handle.clone()}

  pub fn capture_frame<T : AsRef<dyn VideoBuffer>>(&self,
                                                   frame : &VideoFrame<T>) {
    let mut inner = self.inner.lock();
    inner.captured_frames += 1;

    let mut builder = vf_sys::ffi::new_video_frame_builder();
    builder.pin_mut().set_rotation(frame.rotation.into());
    builder.pin_mut().set_video_frame_buffer(
        frame.buffer.as_ref().sys_handle());

    if frame
      .timestamp_us == 0 {
        // If the timestamp is set to 0, default to now
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        builder.pin_mut().set_timestamp_us(now.as_micros() as i64);
      }
    else {
      builder.pin_mut().set_timestamp_us(frame.timestamp_us);
    }

    self.sys_handle.on_captured_frame(&builder.pin_mut().build());
  }

  pub fn video_resolution(&self) -> VideoResolution {
    self.sys_handle.video_resolution().into()
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use tokio::sync::mpsc;

#[tokio::test]
  async fn create_video_native_sink() {}
}