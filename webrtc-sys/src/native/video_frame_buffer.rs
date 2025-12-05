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

use crate::impl_thread_safety;
use crate::sys::{self, *};
use std::pin::Pin;

use super::yuv_helper;
use super::video_frame::{*};
use super::super::video_frame as vf;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i32)]
pub enum VideoFrameBufferType {
  Native,
  I420,
  I420A,
  I422,
  I444,
  I010,
  NV12,
}

impl From<VideoFrameBufferType> for sys::lkVideoFrameBufferType {
  fn from(options : VideoFrameBufferType) -> Self {
    match options {
      VideoFrameBufferType::Native =
          > sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_NATIVE,
      VideoFrameBufferType::I420 =
          > sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I420,
      VideoFrameBufferType::I420A =
          > sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I420A,
      VideoFrameBufferType::I422 =
          > sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I422,
      VideoFrameBufferType::I444 =
          > sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I444,
      VideoFrameBufferType::I010 =
          > sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I010,
      VideoFrameBufferType::NV12 =
          > sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_NV12,
    }
  }
}

impl From< sys::lkVideoFrameBufferType> for  VideoFrameBufferType{
  fn from(options : sys::lkVideoFrameBufferType) -> Self {
    match options {
      sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_NATIVE =
          > VideoFrameBufferType::Native,
      sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I420 =
          > VideoFrameBufferType::I420,
      sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I420A =
          > VideoFrameBufferType::I420A,
      sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I422 =
          > VideoFrameBufferType::I422,
      sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I444 =
          > VideoFrameBufferType::I444,
      sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I010 =
          > VideoFrameBufferType::I010,
      sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_NV12 =
          > VideoFrameBufferType::NV12,
    }
  }
}

/// We don't use vf::VideoFrameBuffer trait for the types inside this module to
/// avoid confusion because directly using platform specific types is not valid
/// (e.g user callback) All the types inside this module are only used
/// internally. For public types, see the top level video_frame.rs

pub fn
new_video_frame_buffer(mut ffi : sys::RefCounted<sys::lkVideoFrameBuffer>, )
    -> Box<dyn vf::VideoBuffer + Send + Sync> {
  unsafe {
    let buffer_type = sys::lkVideoFrameBufferGetType(ffi.as_ptr());
    match buffer_type {
      sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_NATIVE =>{
          Box::new (vf::native::NativeBuffer{
            handle : NativeBuffer{sys_handle}
          })} sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I420 =
          > Box::new (vf::I420Buffer{
              ffi : I420Buffer{sys_handle : sys_handle.pin_mut().get_i420()},
            }),
              sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I420A =
                  >
                  Box::new (vf::I420ABuffer{
                    handle :
                    I420ABuffer{sys_handle : sys_handle.pin_mut().get_i420a()},
                  }),
              sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I422 =
                  > Box::new (vf::I422Buffer{
                      handle :
                      I422Buffer{sys_handle : sys_handle.pin_mut().get_i422()},
                    }),
              sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I444 =
                  > Box::new (vf::I444Buffer{
                      handle :
                      I444Buffer{sys_handle : sys_handle.pin_mut().get_i444()},
                    }),
              sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_I010 =
                  > Box::new (vf::I010Buffer{
                      handle :
                      I010Buffer{sys_handle : sys_handle.pin_mut().get_i010()},
                    }),
              sys::lkVideoFrameBufferType::LK_VIDEO_FRAME_BUFFER_TYPE_NV12 =
                  > Box::new (vf::NV12Buffer{
                      handle :
                      NV12Buffer{sys_handle : sys_handle.pin_mut().get_nv12()},
                    }),
              _ = > unreachable !(),
    }
  }
}

pub trait PlanarYuvBuffer {
  fn chroma_width(&self)->i32;
  fn chroma_height(&self)->i32;
  fn stride_y(&self)->i32;
  fn stride_u(&self)->i32;
  fn stride_v(&self)->i32;
}

pub trait PlanarYuv8Buffer {
  fn data_y(&self)->& [u8];
  fn data_u(&self)->& [u8];
  fn data_v(&self)->& [u8];
}

pub trait PlanarYuv16BBuffer {
  fn data_y(&self)->& [u16];
  fn data_u(&self)->& [u16];
  fn data_v(&self)->& [u16];
}

pub trait BiplanarYuvBuffer {
  fn chroma_width(&self)->i32;
  fn chroma_height(&self)->i32;
  fn stride_y(&self)->i32;
  fn stride_uv(&self)->i32;
}

pub trait BiplanarYuv8Buffer {
  fn data_y(&self)->& [u8];
  fn data_uv(&self)->& [u8];
}

pub struct VideoFrameBuffer {
  ffi : sys::RefCounted<sys::lkRefCountedObject>,
}

impl VideoFrameBuffer {
  pub fn buffer_type(self : &VideoFrameBuffer) -> VideoFrameBufferType {
    unsafe {
      let t = sys::lkVideoFrameBufferGetType(self.ffi.as_ptr());
      t.into()
    }
  }
  pub fn width(self : &VideoFrameBuffer) -> u32 {
    unsafe {
      sys::lkVideoFrameBufferGetWidth(self.ffi.as_ptr()) as u32
    }
  }
  pub fn height(self : &VideoFrameBuffer) -> u32 {
    unsafe {
      sys::lkVideoFrameBufferGetHeight(self.ffi.as_ptr()) as u32
    }
  }

  /// # SAFETY
  /// If the buffer type is I420, the buffer must be cloned before
  pub fn to_i420(self : &VideoFrameBuffer) -> I420Buffer {
    unsafe {
      let lk_i420 = sys::lkVideoFrameBufferToI420(self.ffi.as_ptr());
      I420Buffer {
      ffi:
        sys::RefCounted::from_raw(lk_i420),
      }
    }
  }

  /// # SAFETY
  /// The functions require ownership
  pub fn get_i420(self : Pin<&mut VideoFrameBuffer>) -> I420Buffer {
    unsafe {
      let lk_i420 = sys::lkVideoFrameBufferGetI420(self.ffi.as_ptr());
      I420Buffer {
      ffi:
        sys::RefCounted::from_raw(lk_i420),
      }
    }
  }
  pub fn get_i420a(self : Pin<&mut VideoFrameBuffer>) -> I420ABuffer {
    unsafe {
      let lk_i420a = sys::lkVideoFrameBufferGetI420A(self.ffi.as_ptr());
      I420ABuffer {
      ffi:
        sys::RefCounted::from_raw(lk_i420a),
      }
    }
  }

  pub fn get_i422(self : Pin<&mut VideoFrameBuffer>) -> I422Buffer {
    unsafe {
      let lk_i422 = sys::lkVideoFrameBufferGetI422(self.ffi.as_ptr());
      I422Buffer {
      ffi:
        sys::RefCounted::from_raw(lk_i422),
      }
    }
  }

  pub fn get_i444(self : Pin<&mut VideoFrameBuffer>) -> I444Buffer {
    unsafe {
      let lk_i444 = sys::lkVideoFrameBufferGetI444(self.ffi.as_ptr());
      I444Buffer {
      ffi:
        sys::RefCounted::from_raw(lk_i444),
      }
    }
  }
  pub fn get_i010(self : Pin<&mut VideoFrameBuffer>) -> I010Buffer {
    unsafe {
      let lk_i010 = sys::lkVideoFrameBufferGetI010(self.ffi.as_ptr());
      I010Buffer {
      ffi:
        sys::RefCounted::from_raw(lk_i010),
      }
    }
  }
  pub fn get_nv12(self : Pin<&mut VideoFrameBuffer>) -> NV12Buffer {
    unsafe {
      let lk_nv12 = sys::lkVideoFrameBufferGetNV12(self.ffi.as_ptr());
      NV12Buffer {
      ffi:
        sys::RefCounted::from_raw(lk_nv12),
      }
    }
  }
}

/*
  pub struct PlanarYuvBuffer {
    video_frame_buffer_methods !();
    pub fn chroma_width(self : &PlanarYuvBuffer)->u32;
    pub fn chroma_height(self : &PlanarYuvBuffer)->u32;
    pub fn stride_y(self : &PlanarYuvBuffer)->u32;
    pub fn stride_u(self : &PlanarYuvBuffer)->u32;
    pub fn stride_v(self : &PlanarYuvBuffer)->u32;
  }

  pub struct PlanarYuv8Buffer {
    video_frame_buffer_methods !();
    pub fn data_y(self : &PlanarYuv8Buffer)->*const u8;
    pub fn data_u(self : &PlanarYuv8Buffer)->*const u8;
    pub fn data_v(self : &PlanarYuv8Buffer)->*const u8;
  }

  pub struct BiplanarYuvBuffer {
    video_frame_buffer_methods !();
    pub fn chroma_width(self : &BiplanarYuvBuffer)->u32;
    pub fn chroma_height(self : &BiplanarYuvBuffer)->u32;
    pub fn stride_y(self : &BiplanarYuvBuffer)->u32;
    pub fn stride_uv(self : &BiplanarYuvBuffer)->u32;
  }

  pub struct PlanarYuv16BBuffer {
    video_frame_buffer_methods !();
    pub fn data_y(self : &PlanarYuv16BBuffer)->*const u16;
    pub fn data_u(self : &PlanarYuv16BBuffer)->*const u16;
    pub fn data_v(self : &PlanarYuv16BBuffer)->*const u16;
  }

  pub struct BiplanarYuv8Buffer {
    video_frame_buffer_methods !();
    pub fn data_y(self : &BiplanarYuv8Buffer)->*const u8;
    pub fn data_uv(self : &BiplanarYuv8Buffer)->*const u8;
  }


pub fn copy_i420_buffer(i420 : &I420Buffer)->I420Buffer;

pub fn new_i420_buffer(width : i32,
                       height : i32,
                       stride_y : i32,
                       stride_u : i32,
                       stride_v : i32, )
    ->I420Buffer;

pub fn new_i422_buffer(width : i32,
                       height : i32,
                       stride_y : i32,
                       stride_u : i32,
                       stride_v : i32, )
    ->I422Buffer;

pub fn new_i444_buffer(width : i32,
                       height : i32,
                       stride_y : i32,
                       stride_u : i32,
                       stride_v : i32, )
    ->I444Buffer;

pub fn new_i010_buffer(width : i32,
                       height : i32,
                       stride_y : i32,
                       stride_u : i32,
                       stride_v : i32, )
    ->I010Buffer;

pub fn new_nv12_buffer(width : i32,
                       height : i32,
                       stride_y : i32,
                       stride_uv : i32, )
    ->NV12Buffer;

pub fn new_native_buffer_from_platform_image_buffer(
    platform_native_buffer : *mut PlatformImageBuffer, ) ->
VideoFrameBuffer;

pub fn native_buffer_to_platform_image_buffer(buffer : &VideoFrameBuffer >,
)
        ->*mut PlatformImageBuffer;

fn yuv_to_vfb(yuv : * const PlanarYuvBuffer) -> * const VideoFrameBuffer;
fn biyuv_to_vfb(yuv : * const BiplanarYuvBuffer) -> * const VideoFrameBuffer;
fn yuv8_to_yuv(yuv8 : * const PlanarYuv8Buffer) -> * const PlanarYuvBuffer;
fn yuv16b_to_yuv(yuv16b : * const PlanarYuv16BBuffer)
    -> * const PlanarYuvBuffer;
fn biyuv8_to_biyuv(biyuv8 : * const BiplanarYuv8Buffer)
    -> * const BiplanarYuvBuffer;
fn i420_to_yuv8(i420 : * const I420Buffer) -> * const PlanarYuv8Buffer;
fn i420a_to_yuv8(i420a : * const I420ABuffer) -> * const PlanarYuv8Buffer;
fn i422_to_yuv8(i422 : * const I422Buffer) -> * const PlanarYuv8Buffer;
fn i444_to_yuv8(i444 : * const I444Buffer) -> * const PlanarYuv8Buffer;
fn i010_to_yuv16b(i010 : * const I010Buffer) -> * const PlanarYuv16BBuffer;
fn nv12_to_biyuv8(nv12 : * const NV12Buffer) -> * const BiplanarYuv8Buffer;

fn _unique_video_frame_buffer() -> VideoFrameBuffer;

impl_thread_safety !(VideoFrameBuffer, Send + Sync);
impl_thread_safety !(PlanarYuvBuffer, Send + Sync);
impl_thread_safety !(PlanarYuv8Buffer, Send + Sync);
impl_thread_safety !(PlanarYuv16BBuffer, Send + Sync);
impl_thread_safety !(BiplanarYuvBuffer, Send + Sync);
impl_thread_safety !(BiplanarYuv8Buffer, Send + Sync);
impl_thread_safety !(I420Buffer, Send + Sync);
impl_thread_safety !(I420ABuffer, Send + Sync);
impl_thread_safety !(I422Buffer, Send + Sync);
impl_thread_safety !(I444Buffer, Send + Sync);
impl_thread_safety !(I010Buffer, Send + Sync);
impl_thread_safety !(NV12Buffer, Send + Sync);
*/