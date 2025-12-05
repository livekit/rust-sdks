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

use std::fmt::Debug;

use thiserror::Error;
use crate::sys::{self, *};
use super::yuv_helper;

// Add this import or definition for VideoFormatType
use crate::native::VideoFormatType;

macro_rules !recursive_cast {
  ($ptr : expr $(, $fnc : ident)*) =>{{let ptr = $ptr;
  $(let ptr = vfb_sys::ffi::$fnc(ptr);) * ptr
}
}
;
}

pub struct NativeBuffer {
  ffi : sys::RefCounted<sys::lkVideoFrameBuffer>,
}

pub struct I420Buffer {
  ffi : sys::RefCounted<sys::lkI420Buffer>,
}

pub struct I420ABuffer {
  ffi : sys::RefCounted<sys::lkI420ABuffer>,
}

pub struct I422Buffer {
  ffi : sys::RefCounted<sys::lkI422Buffer>,
}

pub struct I444Buffer {
  ffi : sys::RefCounted<sys::lkI444Buffer>,
}

pub struct I010Buffer {
  ffi : sys::RefCounted<sys::lkI010Buffer>,
}

pub struct NV12Buffer {
  ffi : sys::RefCounted<sys::lkNV12Buffer>,
}

macro_rules !impl_to_argb {
  (I420Buffer[$($variant:ident:$fnc : ident), +], $format : ident,
   $self : ident, $dst : ident, $dst_stride : ident, $dst_width : ident,
   $dst_height : ident) =>{match $format{
      $(VideoFormatType::$variant = > {
        let(data_y, data_u, data_v) = $self.data();
        yuv_helper::$fnc(data_y, $self.stride_y(), data_u, $self.stride_u(),
                         data_v, $self.stride_v(), $dst, $dst_stride,
                         $dst_width, $dst_height, )
      }) + }};
  (I420ABuffer) => {
    todo !();
  }
}

#[allow(unused_unsafe)]
impl NativeBuffer {
#[cfg(any(target_os = "macos", target_os = "ios"))]
  pub unsafe fn from_cv_pixel_buffer(cv_pixel_buffer : *mut std::ffi::c_void, )
      -> NativeBuffer {
    let ffi = unsafe {
      sys::lkNewNativeBufferFromPlatformImageBuffer(
          cv_pixel_buffer as * mut _, );
    };
    Self {
    ffi:
      sys::RefCounted::from_raw(ffi)
    }
  }
#[cfg(any(target_os = "macos", target_os = "ios"))]
  pub fn get_cv_pixel_buffer(&self)->*mut std::ffi::c_void {
    unsafe {
      sys::lkNativeBufferToPlatformImageBuffer(self.ffi.as_ptr()) as* mut _
    }
  }

  pub fn sys_handle(&self) -> & VideoFrameBuffer{&self.sys_handle}

  pub fn width(&self)
      ->u32{self.sys_handle.width()}

  pub fn height(&self)
      ->u32{self.sys_handle.height()}

  pub fn to_i420(&self)
      ->I420Buffer {
    I420Buffer {
    sys_handle:
      unsafe {
        self.sys_handle.to_i420()
      }
    }
  }

  pub fn to_argb(&self, format : VideoFormatType, dst : &mut[u8],
                 dst_stride : u32, dst_width : i32, dst_height : i32, ) {
    self.to_i420().to_argb(format, dst, dst_stride, dst_width, dst_height)
  }
}

impl I420Buffer {
  pub fn with_strides(width : u32, height : u32, stride_y : u32, stride_u : u32,
                      stride_v : u32, ) -> I420Buffer {
    let ffi = unsafe {
      sys::lkI420BufferNew(width, height, stride_y, stride_u, stride_v, );
    };
    I420Buffer {
    ffi:
      sys::RefCounted::from_raw(ffi)
    }
  }

  pub fn new (width : u32, height : u32)
      ->I420Buffer{Self::with_strides(width, height, width, (width + 1) / 2,
                                      (width + 1) / 2)}

  pub fn chroma_width(&self)
      ->u32 {
    unsafe {
      sys::lkI420BufferGetChromaWidth(self.ffi.as_ptr())
    }
  }

  pub fn chroma_height(&self) -> u32 {
    unsafe {
      sys::lkI420BufferGetChromaHeight(self.ffi.as_ptr())
    }
  }

  pub fn strides(&self) -> (u32, u32, u32) {
    let stride_y = unsafe {
      sys::lkI420BufferGetStrideY(self.ffi.as_ptr());
    };
    let stride_u = unsafe {
      sys::lkI420BufferGetStrideU(self.ffi.as_ptr());
    };
    let stride_v = unsafe {
      sys::lkI420BufferGetStrideV(self.ffi.as_ptr());
    };
    (stride_y, stride_u, stride_v)
  }

  pub fn data(&self) -> (&[u8], &[u8], &[u8]) {
    let data_y = unsafe {
      let ptr = sys::lkI420BufferGetDataY(self.ffi.as_ptr());
      let len = (self.stride_y() * self.height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };
    let data_u = unsafe {
      let ptr = sys::lkI420BufferGetDataU(self.ffi.as_ptr());
      let len = (self.stride_u() * self.chroma_height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };

    let data_v = unsafe {
      let ptr = sys::lkI420BufferGetDataV(self.ffi.as_ptr());
      let len = (self.stride_v() * self.chroma_height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };
    (data_y, data_u, data_v)
  }

  pub fn data_mut(&mut self) -> (&mut[u8], &mut[u8], &mut[u8]) {
    let(data_y, data_u, data_v) = self.data();
    unsafe {
      (std::slice::from_raw_parts_mut(data_y.as_ptr() as * mut u8,
                                      data_y.len()),
       std::slice::from_raw_parts_mut(data_u.as_ptr() as * mut u8,
                                      data_u.len()),
       std::slice::from_raw_parts_mut(data_v.as_ptr() as * mut u8,
                                      data_v.len()), )
    }
  }

  pub fn to_i420(&self) -> I420Buffer {
    I420Buffer {
    ffi:
      unsafe {
        sys::lkVideoFrameBufferToI420(self.ffi.as_ptr())
      }
    }
  }

  pub fn scale(&mut self, scaled_width : i32, scaled_height : i32)
      -> I420Buffer {
    let ffi = unsafe{
        sys::lkI420BufferScale(self.ffi.as_ptr(), scaled_width, scaled_height)};
    I420Buffer {
    ffi:
      sys::RefCounted::from_raw(ffi)
    }
  }

  pub fn to_argb(&self, format : VideoFormatType, dst : &mut[u8],
                 dst_stride : u32, dst_width : i32, dst_height : i32, ) {
    impl_to_argb !(I420Buffer
                   [ARGB:i420_to_argb,
                       BGRA:i420_to_bgra, ABGR:i420_to_abgr, RGBA:i420_to_rgba],
                   format, self, dst, dst_stride, dst_width, dst_height)
  }
}

impl I420ABuffer {
  pub fn chroma_width(&self) -> u32 {
    unsafe {
      sys::lkI420ABufferGetChromaWidth(self.ffi.as_ptr())
    }
  }

  pub fn chroma_height(&self) -> u32 {
    unsafe {
      sys::lkI420ABufferGetChromaHeight(self.ffi.as_ptr())
    }
  }

  pub fn strides(&self) -> (u32, u32, u32, u32) {
    let stride_y = unsafe {
      sys::lkI420ABufferGetStrideY(self.ffi.as_ptr());
    };
    let stride_u = unsafe {
      sys::lkI420ABufferGetStrideU(self.ffi.as_ptr());
    };
    let stride_v = unsafe {
      sys::lkI420ABufferGetStrideV(self.ffi.as_ptr());
    };
    let stride_a = unsafe {
      sys::lkI420ABufferGetStrideA(self.ffi.as_ptr());
    };

    (stride_y, stride_u, stride_v, stride_a)
  }
#[allow(clippy::type_complexity)]
  pub fn data(&self) -> (&[u8], &[u8], &[u8], Option<&[u8]>) {
    let data_y = unsafe {
      let ptr = sys::lkI420ABufferGetDataY(self.ffi.as_ptr());
      let len = (self.stride_y() * self.height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };
    let data_u = unsafe {
      let ptr = sys::lkI420ABufferGetDataU(self.ffi.as_ptr());
      let len = (self.stride_u() * self.chroma_height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };
    let data_v = unsafe {
      let ptr = sys::lkI420ABufferGetDataV(self.ffi.as_ptr());
      let len = (self.stride_v() * self.chroma_height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };
    let data_a = unsafe {
      let ptr = sys::lkI420ABufferGetDataA(self.ffi.as_ptr());
      if ptr
        .is_null() {
          None
        }
      else {
        let len = (self.stride_a() * self.height()) as usize;
        Some(std::slice::from_raw_parts(ptr, len))
      }
    };
    (data_y, data_u, data_v, data_a)
  }
#[allow(clippy::type_complexity)]
  pub fn data_mut(&self) -> (&mut[u8], &mut[u8], &mut[u8], Option<&mut[u8]>) {
    let(data_y, data_u, data_v, data_a) = self.data();
    unsafe {
      (std::slice::from_raw_parts_mut(data_y.as_ptr() as * mut u8,
                                      data_y.len()),
       std::slice::from_raw_parts_mut(data_u.as_ptr() as * mut u8,
                                      data_u.len()),
       std::slice::from_raw_parts_mut(data_v.as_ptr() as * mut u8,
                                      data_v.len()),
       data_a.map(| data_a |
                  {std::slice::from_raw_parts_mut(data_a.as_ptr() as * mut u8,
                                                  data_a.len())}), )
    }
  }

  pub fn to_i420(&self) -> I420Buffer {
    I420Buffer {
    ffi:
      unsafe {
        sys::lkVideoFrameBufferToI420(self.ffi.as_ptr())
      }
    }
  }

  pub fn scale(&mut self, scaled_width : i32, scaled_height : i32)
      -> I420ABuffer {
    let ffi = unsafe{sys::lkI420ABufferScale(self.ffi.as_ptr(), scaled_width,
                                             scaled_height)};
    I420ABuffer {
    ffi:
      sys::RefCounted::from_raw(ffi)
    }
  }

  pub fn to_argb(&self, format : VideoFormatType, dst : &mut[u8],
                 dst_stride : u32, dst_width : i32, dst_height : i32, ) {
    self.to_i420().to_argb(format, dst, dst_stride, dst_width, dst_height)
  }
}

impl I422Buffer {
  pub fn with_strides(width : u32, height : u32, stride_y : u32, stride_u : u32,
                      stride_v : u32, ) -> I422Buffer {
    let ffi = unsafe{
        sys::lkI422BufferNew(width, height, stride_y, stride_u, stride_v)};
    I422Buffer {
    ffi:
      sys::RefCounted::from_raw(ffi)
    }
  }

  pub fn new (width : u32, height : u32)
      ->I422Buffer{Self::with_strides(width, height, width, (width + 1) / 2,
                                      (width + 1) / 2)}

  pub fn chroma_width(&self)
      ->u32 {
    unsafe {
      sys::lkI422BufferGetChromaWidth(self.ffi.as_ptr())
    }
  }

  pub fn chroma_height(&self) -> u32 {
    unsafe {
      sys::lkI422BufferGetChromaHeight(self.ffi.as_ptr())
    }
  }

  pub fn strides(&self) -> (u32, u32, u32) {
    let stride_y = unsafe {
      sys::lkI422BufferGetStrideY(self.ffi.as_ptr());
    };
    let stride_u = unsafe {
      sys::lkI422BufferGetStrideU(self.ffi.as_ptr());
    };
    let stride_v = unsafe {
      sys::lkI422BufferGetStrideV(self.ffi.as_ptr());
    };
    (stride_y, stride_u, stride_v)
  }

  pub fn data(&self) -> (&[u8], &[u8], &[u8]) {
    let data_y = unsafe {
      let ptr = sys::lkI422BufferGetDataY(self.ffi.as_ptr());
      let len = (self.stride_y() * self.height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };
    let data_u = unsafe {
      let ptr = sys::lkI422BufferGetDataU(self.ffi.as_ptr());
      let len = (self.stride_u() * self.chroma_height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };

    let data_v = unsafe {
      let ptr = sys::lkI422BufferGetDataV(self.ffi.as_ptr());
      let len = (self.stride_v() * self.chroma_height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };
    (data_y, data_u, data_v)
  }

  pub fn data_mut(&mut self) -> (&mut[u8], &mut[u8], &mut[u8]) {
    let(data_y, data_u, data_v) = self.data();
    unsafe {
      (std::slice::from_raw_parts_mut(data_y.as_ptr() as * mut u8,
                                      data_y.len()),
       std::slice::from_raw_parts_mut(data_u.as_ptr() as * mut u8,
                                      data_u.len()),
       std::slice::from_raw_parts_mut(data_v.as_ptr() as * mut u8,
                                      data_v.len()), )
    }
  }

  pub fn to_i420(&self) -> I420Buffer {
    I420Buffer {
    ffi:
      unsafe {
        sys::lkVideoFrameBufferToI420(self.ffi.as_ptr())
      }
    }
  }

  pub fn scale(&mut self, scaled_width : i32, scaled_height : i32)
      -> I422Buffer {
    let ffi = unsafe{
        sys::lkI422BufferScale(self.ffi.as_ptr(), scaled_width, scaled_height)};
    I422Buffer {
    ffi:
      sys::RefCounted::from_raw(ffi)
    }
  }

  pub fn to_argb(&self, format : VideoFormatType, dst : &mut[u8],
                 dst_stride : u32, dst_width : i32, dst_height : i32, ) {
    self.to_i420().to_argb(format, dst, dst_stride, dst_width, dst_height)
  }
}

impl I444Buffer {
  pub fn with_strides(width : u32, height : u32, stride_y : u32, stride_u : u32,
                      stride_v : u32, ) -> I444Buffer {
    let ffi = unsafe{
        sys::lkI444BufferNew(width, height, stride_y, stride_u, stride_v)};
    I444Buffer {
    ffi:
      sys::RefCounted::from_raw(ffi)
    }
  }

  pub fn new (width : u32, height : u32)
      ->I444Buffer{Self::with_strides(width, height, width, width, width)}

  pub fn chroma_width(&self)
      ->u32 {
    unsafe {
      sys::lkI444BufferGetChromaWidth(self.ffi.as_ptr())
    }
  }

  pub fn chroma_height(&self) -> u32 {
    unsafe {
      sys::lkI444BufferGetChromaHeight(self.ffi.as_ptr())
    }
  }

  pub fn strides(&self) -> (u32, u32, u32) {
    let stride_y = unsafe {
      sys::lkI444BufferGetStrideY(self.ffi.as_ptr());
    };
    let stride_u = unsafe {
      sys::lkI444BufferGetStrideU(self.ffi.as_ptr());
    };
    let stride_v = unsafe {
      sys::lkI444BufferGetStrideV(self.ffi.as_ptr());
    };
    (stride_y, stride_u, stride_v)
  }

  pub fn data(&self) -> (&[u8], &[u8], &[u8]) {
    let data_y = unsafe {
      let ptr = sys::lkI444BufferGetDataY(self.ffi.as_ptr());
      let len = (self.stride_y() * self.height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };
    let data_u = unsafe {
      let ptr = sys::lkI444BufferGetDataU(self.ffi.as_ptr());
      let len = (self.stride_u() * self.chroma_height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };

    let data_v = unsafe {
      let ptr = sys::lkI444BufferGetDataV(self.ffi.as_ptr());
      let len = (self.stride_v() * self.chroma_height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };
    (data_y, data_u, data_v)
  }

  pub fn data_mut(&mut self) -> (&mut[u8], &mut[u8], &mut[u8]) {
    let(data_y, data_u, data_v) = self.handle.data();
    unsafe {
      (std::slice::from_raw_parts_mut(data_y.as_ptr() as * mut u8,
                                      data_y.len()),
       std::slice::from_raw_parts_mut(data_u.as_ptr() as * mut u8,
                                      data_u.len()),
       std::slice::from_raw_parts_mut(data_v.as_ptr() as * mut u8,
                                      data_v.len()), )
    }
  }

  pub fn to_i420(&self) -> I420Buffer {
    I420Buffer {
    ffi:
      unsafe {
        sys::lkVideoFrameBufferToI420(self.ffi.as_ptr())
      }
    }
  }

  pub fn scale(&mut self, scaled_width : i32, scaled_height : i32)
      -> I444Buffer {
    let ffi = unsafe{
        sys::lkI444BufferScale(self.ffi.as_ptr(), scaled_width, scaled_height)};
    I444Buffer {
    ffi:
      sys::RefCounted::from_raw(ffi)
    }
  }

  pub fn to_argb(&self, format : VideoFormatType, dst : &mut[u8],
                 dst_stride : u32, dst_width : i32, dst_height : i32, ) {
    self.to_i420().to_argb(format, dst, dst_stride, dst_width, dst_height)
  }
}

impl I010Buffer {
  pub fn with_strides(width : u32, height : u32, stride_y : u32, stride_u : u32,
                      stride_v : u32, ) -> I010Buffer {
    let ffi = unsafe{
        sys::lkI010BufferNew(width, height, stride_y, stride_u, stride_v)};

    I010Buffer {
    ffi:
      sys::RefCounted::from_raw(ffi)
    }
  }

  pub fn new (width : u32, height : u32)
      ->I010Buffer{Self::with_strides(width, height, width, (width + 1) / 2,
                                      (width + 1) / 2)}

  pub fn chroma_width(&self)
      ->u32 {
    unsafe {
      sys::lkI010BufferGetChromaWidth(self.ffi.as_ptr())
    }
  }

  pub fn chroma_height(&self) -> u32 {
    unsafe {
      sys::lkI010BufferGetChromaHeight(self.ffi.as_ptr())
    }
  }

  pub fn strides(&self) -> (u32, u32, u32) {
    let stride_y = unsafe {
      sys::lkI010BufferGetStrideY(self.ffi.as_ptr());
    };
    let stride_u = unsafe {
      sys::lkI010BufferGetStrideU(self.ffi.as_ptr());
    };
    let stride_v = unsafe {
      sys::lkI010BufferGetStrideV(self.ffi.as_ptr());
    };
    (stride_y, stride_u, stride_v)
  }

  pub fn data(&self) -> (&[u16], &[u16], &[u16]) {
    let data_y = unsafe {
      let ptr = sys::lkI010BufferGetDataY(self.ffi.as_ptr());
      let len = (self.stride_y() * self.height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };
    let data_u = unsafe {
      let ptr = sys::lkI010BufferGetDataU(self.ffi.as_ptr());
      let len = (self.stride_u() * self.chroma_height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };

    let data_v = unsafe {
      let ptr = sys::lkI010BufferGetDataV(self.ffi.as_ptr());
      let len = (self.stride_v() * self.chroma_height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };
    (data_y, data_u, data_v)
  }

  pub fn data_mut(&mut self) -> (&mut[u16], &mut[u16], &mut[u16]) {
    let(data_y, data_u, data_v) = self.data();
    unsafe {
      (std::slice::from_raw_parts_mut(data_y.as_ptr() as * mut u16,
                                      data_y.len()),
       std::slice::from_raw_parts_mut(data_u.as_ptr() as * mut u16,
                                      data_u.len()),
       std::slice::from_raw_parts_mut(data_v.as_ptr() as * mut u16,
                                      data_v.len()), )
    }
  }

  pub fn to_i420(&self) -> I420Buffer {
    I420Buffer {
    ffi:
      unsafe {
        sys::lkVideoFrameBufferToI420(self.ffi.as_ptr())
      }
    }
  }

  pub fn scale(&mut self, scaled_width : i32, scaled_height : i32)
      -> I010Buffer {
    let ffi = unsafe{
        sys::lkI010BufferScale(self.ffi.as_ptr(), scaled_width, scaled_height)};
    I010Buffer {
    ffi:
      sys::RefCounted::from_raw(ffi)
    }
  }

  pub fn to_argb(&self, format : VideoFormatType, dst : &mut[u8],
                 dst_stride : u32, dst_width : i32, dst_height : i32, ) {
    self.to_i420().to_argb(format, dst, dst_stride, dst_width, dst_height)
  }
}

impl NV12Buffer {
  pub fn with_strides(width : u32, height : u32, stride_y : u32,
                      stride_uv : u32) -> NV12Buffer {
    let ffi = unsafe{sys::lkNV12BufferNew(width, height, stride_y, stride_uv)};
    NV12Buffer {
    ffi:
      sys::RefCounted::from_raw(ffi)
    }
  }

  pub fn new (width : u32, height : u32)
      ->NV12Buffer{Self::with_strides(width, height, width, width + width % 2)}

  pub fn chroma_width(&self)
      ->u32 {
    unsafe {
      sys::lkNV12BufferGetChromaWidth(self.ffi.as_ptr())
    }
  }

  pub fn chroma_height(&self) -> u32 {
    unsafe {
      sys::lkNV12BufferGetChromaHeight(self.ffi.as_ptr())
    }
  }

  pub fn strides(&self) -> (u32, u32) {
    let stride_y = unsafe {
      sys::lkNV12BufferGetStrideY(self.ffi.as_ptr());
    };
    let stride_uv = unsafe {
      sys::lkNV12BufferGetStrideUV(self.ffi.as_ptr());
    };
    (stride_y, stride_uv)
  }

  pub fn data(&self) -> (&[u8], &[u8]) {
    let data_y = unsafe {
      let ptr = sys::lkNV12BufferGetDataY(self.ffi.as_ptr());
      let len = (self.stride_y() * self.height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };
    let data_uv = unsafe {
      let ptr = sys::lkNV12BufferGetDataUV(self.ffi.as_ptr());
      let len = (self.stride_uv() * self.chroma_height()) as usize;
      std::slice::from_raw_parts(ptr, len)
    };
    (data_y, data_uv)
  }

  pub fn data_mut(&mut self) -> (&mut[u8], &mut[u8]) {
    let(data_y, data_uv) = self.data();
    unsafe {
      (std::slice::from_raw_parts_mut(data_y.as_ptr() as * mut u8,
                                      data_y.len()),
       std::slice::from_raw_parts_mut(data_uv.as_ptr() as * mut u8,
                                      data_uv.len()), )
    }
  }

  pub fn scale(&mut self, scaled_width : i32, scaled_height : i32)
      -> NV12Buffer {
    let ffi = unsafe{
        sys::lkNV12BufferScale(self.ffi.as_ptr(), scaled_width, scaled_height)};
    NV12Buffer {
    ffi:
      sys::RefCounted::from_raw(ffi)
    }
  }

  pub fn to_i420(&self) -> I420Buffer {
    I420Buffer {
    ffi:
      unsafe {
        sys::lkVideoFrameBufferToI420(self.ffi.as_ptr())
      }
    }
  }

  pub fn to_argb(&self, format : VideoFormatType, dst : &mut[u8],
                 dst_stride : u32, dst_width : i32, dst_height : i32, ) {
    self.to_i420().to_argb(format, dst, dst_stride, dst_width, dst_height)
  }
}
