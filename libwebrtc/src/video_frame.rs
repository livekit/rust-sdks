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
use crate::{native::yuv_helper, sys, video_frame::internal::BufferSealed};
use std::fmt::Debug;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SinkError {
    #[error("platform error: {0}")]
    Platform(String),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VideoRotation {
    VideoRotation0 = 0,
    VideoRotation90 = 90,
    VideoRotation180 = 180,
    VideoRotation270 = 270,
}

impl From<sys::lkVideoRotation> for VideoRotation {
    fn from(value: sys::lkVideoRotation) -> Self {
        match value {
            sys::lkVideoRotation::LK_VIDEO_ROTATION_0 => Self::VideoRotation0,
            sys::lkVideoRotation::LK_VIDEO_ROTATION_90 => Self::VideoRotation90,
            sys::lkVideoRotation::LK_VIDEO_ROTATION_180 => Self::VideoRotation180,
            sys::lkVideoRotation::LK_VIDEO_ROTATION_270 => Self::VideoRotation270,
        }
    }
}

impl From<VideoRotation> for sys::lkVideoRotation {
    fn from(value: VideoRotation) -> Self {
        match value {
            VideoRotation::VideoRotation0 => sys::lkVideoRotation::LK_VIDEO_ROTATION_0,
            VideoRotation::VideoRotation90 => sys::lkVideoRotation::LK_VIDEO_ROTATION_90,
            VideoRotation::VideoRotation180 => sys::lkVideoRotation::LK_VIDEO_ROTATION_180,
            VideoRotation::VideoRotation270 => sys::lkVideoRotation::LK_VIDEO_ROTATION_270,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VideoFormatType {
    ARGB,
    BGRA,
    ABGR,
    RGBA,
}
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VideoBufferType {
    Native,
    I420,
    I420A,
    I422,
    I444,
    I010,
    NV12,
}

impl From<VideoBufferType> for sys::lkVideoBufferType {
    fn from(value: VideoBufferType) -> Self {
        match value {
            VideoBufferType::Native => sys::lkVideoBufferType::LK_VIDEO_BUFFER_TYPE_NATIVE,
            VideoBufferType::I420 => sys::lkVideoBufferType::LK_VIDEO_BUFFER_TYPE_I420,
            VideoBufferType::I420A => sys::lkVideoBufferType::LK_VIDEO_BUFFER_TYPE_I420A,
            VideoBufferType::I422 => sys::lkVideoBufferType::LK_VIDEO_BUFFER_TYPE_I422,
            VideoBufferType::I444 => sys::lkVideoBufferType::LK_VIDEO_BUFFER_TYPE_I444,
            VideoBufferType::I010 => sys::lkVideoBufferType::LK_VIDEO_BUFFER_TYPE_I010,
            VideoBufferType::NV12 => sys::lkVideoBufferType::LK_VIDEO_BUFFER_TYPE_NV12,
        }
    }
}

impl From<sys::lkVideoBufferType> for VideoBufferType {
    fn from(value: sys::lkVideoBufferType) -> Self {
        match value {
            sys::lkVideoBufferType::LK_VIDEO_BUFFER_TYPE_NATIVE => VideoBufferType::Native,
            sys::lkVideoBufferType::LK_VIDEO_BUFFER_TYPE_I420 => VideoBufferType::I420,
            sys::lkVideoBufferType::LK_VIDEO_BUFFER_TYPE_I420A => VideoBufferType::I420A,
            sys::lkVideoBufferType::LK_VIDEO_BUFFER_TYPE_I422 => VideoBufferType::I422,
            sys::lkVideoBufferType::LK_VIDEO_BUFFER_TYPE_I444 => VideoBufferType::I444,
            sys::lkVideoBufferType::LK_VIDEO_BUFFER_TYPE_I010 => VideoBufferType::I010,
            sys::lkVideoBufferType::LK_VIDEO_BUFFER_TYPE_NV12 => VideoBufferType::NV12,
        }
    }
}

pub trait VideoBuffer: internal::BufferSealed + Debug {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn buffer_type(&self) -> VideoBufferType;
    fn ffi(&self) -> sys::RefCounted<sys::lkRefCountedObject>;

    #[cfg(not(target_arch = "wasm32"))]
    fn as_native(&self) -> Option<&NativeBuffer> {
        None
    }

    fn as_i420(&self) -> Option<&I420Buffer> {
        None
    }

    fn as_i420a(&self) -> Option<&I420ABuffer> {
        None
    }

    fn as_i422(&self) -> Option<&I422Buffer> {
        None
    }

    fn as_i444(&self) -> Option<&I444Buffer> {
        None
    }

    fn as_i010(&self) -> Option<&I010Buffer> {
        None
    }

    fn as_nv12(&self) -> Option<&NV12Buffer> {
        None
    }
}

#[derive(Debug)]
pub struct VideoFrame<T>
where
    T: AsRef<dyn VideoBuffer>,
{
    pub rotation: VideoRotation,
    pub timestamp_us: i64, // When the frame was captured in microseconds
    pub buffer: T,
}

pub type BoxVideoBuffer = Box<dyn VideoBuffer>;
pub type BoxVideoFrame = VideoFrame<BoxVideoBuffer>;

pub(crate) mod internal {
    use super::{I420Buffer, VideoBuffer, VideoFormatType};

    pub trait BufferSealed: Send + Sync {
        #[cfg(not(target_arch = "wasm32"))]
        fn buffer(&self) -> &dyn VideoBuffer;

        #[cfg(not(target_arch = "wasm32"))]
        fn to_i420(&self) -> I420Buffer;

        #[cfg(not(target_arch = "wasm32"))]
        fn to_argb(
            &self,
            format: VideoFormatType,
            dst: &mut [u8],
            dst_stride: u32,
            dst_width: i32,
            dst_height: i32,
        );
    }
}

macro_rules! new_buffer_type {
    ($type : ident, $variant : ident, $as : ident) => {
        pub struct $type {
            pub(crate) ffi: sys::RefCounted<sys::lkRefCountedObject>,
        }

        impl internal::BufferSealed for $type {
            #[cfg(not(target_arch = "wasm32"))]
            fn buffer(&self) -> &dyn VideoBuffer {
                self.as_ref()
            }

            #[cfg(not(target_arch = "wasm32"))]
            fn to_i420(&self) -> I420Buffer {
                let i420_ffi = unsafe {
                    sys::lkVideoFrameBufferToI420(self.ffi.as_ptr() as *mut sys::lkVideoFrameBuffer)
                };
                I420Buffer { ffi: unsafe { sys::RefCounted::from_raw(i420_ffi) } }
            }

            #[cfg(not(target_arch = "wasm32"))]
            fn to_argb(
                &self,
                _format: VideoFormatType,
                _dst: &mut [u8],
                _stride: u32,
                _width: i32,
                _height: i32,
            ) {
                todo!()
            }
        }

        impl VideoBuffer for $type {
            fn width(&self) -> u32 {
                let width = unsafe {
                    sys::lkVideoFrameBufferGetWidth(
                        self.ffi.as_ptr() as *mut sys::lkVideoFrameBuffer
                    )
                };
                width
            }

            fn height(&self) -> u32 {
                let height = unsafe { sys::lkVideoFrameBufferGetHeight(self.ffi.as_ptr()) };
                height
            }

            fn buffer_type(&self) -> VideoBufferType {
                VideoBufferType::$variant
            }

            fn ffi(&self) -> sys::RefCounted<sys::lkRefCountedObject> {
                self.ffi.clone()
            }
        }

        impl Debug for $type {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($type))
                    .field("width", &self.width())
                    .field("height", &self.height())
                    .finish()
            }
        }

        impl AsRef<dyn VideoBuffer> for $type {
            fn as_ref(&self) -> &(dyn VideoBuffer + 'static) {
                self
            }
        }
    };
}

new_buffer_type!(I420Buffer, I420, as_i420);
new_buffer_type!(I420ABuffer, I420A, as_i420a);
new_buffer_type!(I422Buffer, I422, as_i422);
new_buffer_type!(I444Buffer, I444, as_i444);
new_buffer_type!(I010Buffer, I010, as_i010);
new_buffer_type!(NV12Buffer, NV12, as_nv12);
new_buffer_type!(NativeBuffer, Native, as_native);

macro_rules! impl_to_argb {
    (I420Buffer [$($variant:ident: $fnc:ident),+], $format:ident, $self:ident, $dst:ident, $dst_stride:ident, $dst_width:ident, $dst_height:ident) => {
        match $format {
        $(
            VideoFormatType::$variant => {
                let (data_y, data_u, data_v) = $self.data();
                unsafe {
                    yuv_helper::$fnc(
                        data_y,
                        $self.stride_y(),
                        data_u,
                        $self.stride_u(),
                        data_v,
                        $self.stride_v(),
                        $dst,
                        $dst_stride,
                        $dst_width,
                        $dst_height,
                    )
                }
            }
        )+
        }
    };
    (I420ABuffer) => {
        todo!();
    }
}

impl I420Buffer {
    pub fn with_strides(
        width: u32,
        height: u32,
        stride_y: u32,
        stride_u: u32,
        stride_v: u32,
    ) -> I420Buffer {
        let ffi = unsafe { sys::lkI420BufferNew(width, height, stride_y, stride_u, stride_v) };
        I420Buffer { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn new(width: u32, height: u32) -> I420Buffer {
        Self::with_strides(width, height, width, (width + 1) / 2, (width + 1) / 2)
    }

    pub fn new_boxed(width: u32, height: u32) -> Box<I420Buffer> {
        Box::new(Self::new(width, height))
    }

    pub fn chroma_width(&self) -> u32 {
        unsafe { sys::lkI420BufferGetChromaWidth(self.ffi.as_ptr()) }
    }

    pub fn chroma_height(&self) -> u32 {
        unsafe { sys::lkI420BufferGetChromaHeight(self.ffi.as_ptr()) }
    }

    pub fn stride_y(&self) -> u32 {
        unsafe { sys::lkI420BufferGetStrideY(self.ffi.as_ptr()) }
    }

    pub fn stride_u(&self) -> u32 {
        unsafe { sys::lkI420BufferGetStrideU(self.ffi.as_ptr()) }
    }

    pub fn stride_v(&self) -> u32 {
        unsafe { sys::lkI420BufferGetStrideV(self.ffi.as_ptr()) }
    }

    pub fn strides(&self) -> (u32, u32, u32) {
        (self.stride_y(), self.stride_u(), self.stride_v())
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

    pub fn data_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        let (data_y, data_u, data_v) = self.data();
        unsafe {
            (
                std::slice::from_raw_parts_mut(data_y.as_ptr() as *mut u8, data_y.len()),
                std::slice::from_raw_parts_mut(data_u.as_ptr() as *mut u8, data_u.len()),
                std::slice::from_raw_parts_mut(data_v.as_ptr() as *mut u8, data_v.len()),
            )
        }
    }
    pub fn scale(&mut self, scaled_width: i32, scaled_height: i32) -> I420Buffer {
        let ffi = unsafe { sys::lkI420BufferScale(self.ffi.as_ptr(), scaled_width, scaled_height) };
        I420Buffer { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn to_argb(
        &self,
        format: VideoFormatType,
        dst: &mut [u8],
        dst_stride: u32,
        dst_width: i32,
        dst_height: i32,
    ) {
        impl_to_argb!(
            I420Buffer
            [
                ARGB: i420_to_argb,
                BGRA: i420_to_bgra,
                ABGR: i420_to_abgr,
                RGBA: i420_to_rgba
            ],
            format, self, dst, dst_stride, dst_width, dst_height
        )
    }
}

impl I420ABuffer {
    pub fn chroma_width(&self) -> u32 {
        unsafe { sys::lkI420ABufferGetChromaWidth(self.ffi.as_ptr()) }
    }

    pub fn chroma_height(&self) -> u32 {
        unsafe { sys::lkI420ABufferGetChromaHeight(self.ffi.as_ptr()) }
    }

    pub fn stride_y(&self) -> u32 {
        unsafe { sys::lkI420ABufferGetStrideY(self.ffi.as_ptr()) }
    }

    pub fn stride_u(&self) -> u32 {
        unsafe { sys::lkI420ABufferGetStrideU(self.ffi.as_ptr()) }
    }

    pub fn stride_v(&self) -> u32 {
        unsafe { sys::lkI420ABufferGetStrideV(self.ffi.as_ptr()) }
    }

    pub fn stride_a(&self) -> u32 {
        unsafe { sys::lkI420ABufferGetStrideA(self.ffi.as_ptr()) }
    }

    pub fn strides(&self) -> (u32, u32, u32, u32) {
        (self.stride_y(), self.stride_u(), self.stride_v(), self.stride_a())
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
            if ptr.is_null() {
                None
            } else {
                let (_, _, _, stride_a) = self.strides();
                let len = (stride_a * self.height()) as usize;
                Some(std::slice::from_raw_parts(ptr, len))
            }
        };
        (data_y, data_u, data_v, data_a)
    }

    #[allow(clippy::type_complexity)]
    pub fn data_mut(&self) -> (&mut [u8], &mut [u8], &mut [u8], Option<&mut [u8]>) {
        let (data_y, data_u, data_v, data_a) = self.data();
        unsafe {
            (
                std::slice::from_raw_parts_mut(data_y.as_ptr() as *mut u8, data_y.len()),
                std::slice::from_raw_parts_mut(data_u.as_ptr() as *mut u8, data_u.len()),
                std::slice::from_raw_parts_mut(data_v.as_ptr() as *mut u8, data_v.len()),
                data_a.map(|data_a| {
                    std::slice::from_raw_parts_mut(data_a.as_ptr() as *mut u8, data_a.len())
                }),
            )
        }
    }

    pub fn scale(&mut self, scaled_width: i32, scaled_height: i32) -> I420ABuffer {
        let ffi =
            unsafe { sys::lkI420ABufferScale(self.ffi.as_ptr(), scaled_width, scaled_height) };
        I420ABuffer { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn to_argb(
        &self,
        format: VideoFormatType,
        dst: &mut [u8],
        dst_stride: u32,
        dst_width: i32,
        dst_height: i32,
    ) {
        self.buffer().to_i420().to_argb(format, dst, dst_stride, dst_width, dst_height)
    }
}

impl I422Buffer {
    pub fn with_strides(
        width: u32,
        height: u32,
        stride_y: u32,
        stride_u: u32,
        stride_v: u32,
    ) -> I422Buffer {
        let ffi = unsafe { sys::lkI422BufferNew(width, height, stride_y, stride_u, stride_v) };
        I422Buffer { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn new(width: u32, height: u32) -> I422Buffer {
        Self::with_strides(width, height, width, (width + 1) / 2, (width + 1) / 2)
    }

    pub fn chroma_width(&self) -> u32 {
        unsafe { sys::lkI422BufferGetChromaWidth(self.ffi.as_ptr()) }
    }

    pub fn chroma_height(&self) -> u32 {
        unsafe { sys::lkI422BufferGetChromaHeight(self.ffi.as_ptr()) }
    }
    pub fn stride_y(&self) -> u32 {
        unsafe { sys::lkI422BufferGetStrideY(self.ffi.as_ptr()) }
    }

    pub fn stride_u(&self) -> u32 {
        unsafe { sys::lkI422BufferGetStrideU(self.ffi.as_ptr()) }
    }

    pub fn stride_v(&self) -> u32 {
        unsafe { sys::lkI422BufferGetStrideV(self.ffi.as_ptr()) }
    }

    pub fn strides(&self) -> (u32, u32, u32) {
        (self.stride_y(), self.stride_u(), self.stride_v())
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

    pub fn data_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        let (data_y, data_u, data_v) = self.data();
        unsafe {
            (
                std::slice::from_raw_parts_mut(data_y.as_ptr() as *mut u8, data_y.len()),
                std::slice::from_raw_parts_mut(data_u.as_ptr() as *mut u8, data_u.len()),
                std::slice::from_raw_parts_mut(data_v.as_ptr() as *mut u8, data_v.len()),
            )
        }
    }

    pub fn scale(&mut self, scaled_width: i32, scaled_height: i32) -> I422Buffer {
        let ffi = unsafe { sys::lkI422BufferScale(self.ffi.as_ptr(), scaled_width, scaled_height) };
        I422Buffer { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn to_argb(
        &self,
        format: VideoFormatType,
        dst: &mut [u8],
        dst_stride: u32,
        dst_width: i32,
        dst_height: i32,
    ) {
        self.buffer().to_i420().to_argb(format, dst, dst_stride, dst_width, dst_height)
    }
}

impl I444Buffer {
    pub fn with_strides(
        width: u32,
        height: u32,
        stride_y: u32,
        stride_u: u32,
        stride_v: u32,
    ) -> I444Buffer {
        let ffi = unsafe { sys::lkI444BufferNew(width, height, stride_y, stride_u, stride_v) };
        I444Buffer { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn new(width: u32, height: u32) -> I444Buffer {
        Self::with_strides(width, height, width, width, width)
    }

    pub fn chroma_width(&self) -> u32 {
        unsafe { sys::lkI444BufferGetChromaWidth(self.ffi.as_ptr()) }
    }

    pub fn chroma_height(&self) -> u32 {
        unsafe { sys::lkI444BufferGetChromaHeight(self.ffi.as_ptr()) }
    }

    pub fn stride_y(&self) -> u32 {
        unsafe { sys::lkI444BufferGetStrideY(self.ffi.as_ptr()) }
    }

    pub fn stride_u(&self) -> u32 {
        unsafe { sys::lkI444BufferGetStrideU(self.ffi.as_ptr()) }
    }

    pub fn stride_v(&self) -> u32 {
        unsafe { sys::lkI444BufferGetStrideV(self.ffi.as_ptr()) }
    }

    pub fn strides(&self) -> (u32, u32, u32) {
        (self.stride_y(), self.stride_u(), self.stride_v())
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

    pub fn data_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        let (data_y, data_u, data_v) = self.data();
        unsafe {
            (
                std::slice::from_raw_parts_mut(data_y.as_ptr() as *mut u8, data_y.len()),
                std::slice::from_raw_parts_mut(data_u.as_ptr() as *mut u8, data_u.len()),
                std::slice::from_raw_parts_mut(data_v.as_ptr() as *mut u8, data_v.len()),
            )
        }
    }

    pub fn scale(&mut self, scaled_width: i32, scaled_height: i32) -> I444Buffer {
        let ffi = unsafe { sys::lkI444BufferScale(self.ffi.as_ptr(), scaled_width, scaled_height) };
        I444Buffer { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn to_argb(
        &self,
        format: VideoFormatType,
        dst: &mut [u8],
        dst_stride: u32,
        dst_width: i32,
        dst_height: i32,
    ) {
        self.buffer().to_i420().to_argb(format, dst, dst_stride, dst_width, dst_height)
    }
}

impl I010Buffer {
    pub fn with_strides(
        width: u32,
        height: u32,
        stride_y: u32,
        stride_u: u32,
        stride_v: u32,
    ) -> I010Buffer {
        let ffi = unsafe { sys::lkI010BufferNew(width, height, stride_y, stride_u, stride_v) };

        I010Buffer { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn new(width: u32, height: u32) -> I010Buffer {
        Self::with_strides(width, height, width, (width + 1) / 2, (width + 1) / 2)
    }

    pub fn chroma_width(&self) -> u32 {
        unsafe { sys::lkI010BufferGetChromaWidth(self.ffi.as_ptr()) }
    }

    pub fn chroma_height(&self) -> u32 {
        unsafe { sys::lkI010BufferGetChromaHeight(self.ffi.as_ptr()) }
    }

    pub fn stride_y(&self) -> u32 {
        unsafe { sys::lkI010BufferGetStrideY(self.ffi.as_ptr()) }
    }

    pub fn stride_u(&self) -> u32 {
        unsafe { sys::lkI010BufferGetStrideU(self.ffi.as_ptr()) }
    }

    pub fn stride_v(&self) -> u32 {
        unsafe { sys::lkI010BufferGetStrideV(self.ffi.as_ptr()) }
    }

    pub fn strides(&self) -> (u32, u32, u32) {
        (self.stride_y(), self.stride_u(), self.stride_v())
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

    pub fn data_mut(&mut self) -> (&mut [u16], &mut [u16], &mut [u16]) {
        let (data_y, data_u, data_v) = self.data();
        unsafe {
            (
                std::slice::from_raw_parts_mut(data_y.as_ptr() as *mut u16, data_y.len()),
                std::slice::from_raw_parts_mut(data_u.as_ptr() as *mut u16, data_u.len()),
                std::slice::from_raw_parts_mut(data_v.as_ptr() as *mut u16, data_v.len()),
            )
        }
    }

    pub fn scale(&mut self, scaled_width: i32, scaled_height: i32) -> I010Buffer {
        let ffi = unsafe { sys::lkI010BufferScale(self.ffi.as_ptr(), scaled_width, scaled_height) };
        I010Buffer { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn to_argb(
        &self,
        format: VideoFormatType,
        dst: &mut [u8],
        dst_stride: u32,
        dst_width: i32,
        dst_height: i32,
    ) {
        self.buffer().to_i420().to_argb(format, dst, dst_stride, dst_width, dst_height)
    }
}

impl NV12Buffer {
    pub fn with_strides(width: u32, height: u32, stride_y: u32, stride_uv: u32) -> NV12Buffer {
        let ffi = unsafe { sys::lkNV12BufferNew(width, height, stride_y, stride_uv) };
        NV12Buffer { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn new(width: u32, height: u32) -> NV12Buffer {
        Self::with_strides(width, height, width, width + width % 2)
    }

    pub fn chroma_width(&self) -> u32 {
        unsafe { sys::lkNV12BufferGetChromaWidth(self.ffi.as_ptr()) }
    }

    pub fn chroma_height(&self) -> u32 {
        unsafe { sys::lkNV12BufferGetChromaHeight(self.ffi.as_ptr()) }
    }

    pub fn stride_y(&self) -> u32 {
        unsafe { sys::lkNV12BufferGetStrideY(self.ffi.as_ptr()) }
    }

    pub fn stride_uv(&self) -> u32 {
        unsafe { sys::lkNV12BufferGetStrideUV(self.ffi.as_ptr()) }
    }

    pub fn strides(&self) -> (u32, u32) {
        (self.stride_y(), self.stride_uv())
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

    pub fn data_mut(&mut self) -> (&mut [u8], &mut [u8]) {
        let (data_y, data_uv) = self.data();
        unsafe {
            (
                std::slice::from_raw_parts_mut(data_y.as_ptr() as *mut u8, data_y.len()),
                std::slice::from_raw_parts_mut(data_uv.as_ptr() as *mut u8, data_uv.len()),
            )
        }
    }

    pub fn scale(&mut self, scaled_width: i32, scaled_height: i32) -> NV12Buffer {
        let ffi = unsafe { sys::lkNV12BufferScale(self.ffi.as_ptr(), scaled_width, scaled_height) };
        NV12Buffer { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }

    pub fn to_argb(
        &self,
        format: VideoFormatType,
        dst: &mut [u8],
        dst_stride: u32,
        dst_width: i32,
        dst_height: i32,
    ) {
        self.buffer().to_i420().to_argb(format, dst, dst_stride, dst_width, dst_height)
    }
}

impl NativeBuffer {
    /// Creates a `NativeBuffer` from a `CVPixelBufferRef` pointer.
    ///
    /// This function does not bump the reference count of the pixel buffer.
    ///
    /// Safety: The given pointer must be a valid `CVPixelBufferRef`.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub unsafe fn from_cv_pixel_buffer(cv_pixel_buffer: *mut std::ffi::c_void) -> NativeBuffer {
        let ffi = unsafe { sys::lkNewNativeBufferFromPlatformImageBuffer(cv_pixel_buffer) };
        NativeBuffer { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }
    /// Returns the `CVPixelBufferRef` that backs this buffer, or `null` if
    /// this buffer is not currently backed by a `CVPixelBufferRef`.
    ///
    /// This function does not bump the reference count of the pixel buffer.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub fn get_cv_pixel_buffer(&self) -> *mut std::ffi::c_void {
        unsafe {
            sys::lkNativeBufferToPlatformImageBuffer(
                self.ffi.as_ptr() as *mut sys::lkVideoFrameBuffer
            )
        }
    }

    pub fn to_argb(
        &self,
        format: VideoFormatType,
        dst: &mut [u8],
        dst_stride: u32,
        dst_width: i32,
        dst_height: i32,
    ) {
        self.buffer().to_i420().to_argb(format, dst, dst_stride, dst_width, dst_height)
    }
}

pub trait VideoFrameBufferExt: VideoBuffer {
    fn to_i420(&self) -> I420Buffer;
    fn to_argb(
        &self,
        format: VideoFormatType,
        dst: &mut [u8],
        dst_stride: u32,
        dst_width: i32,
        dst_height: i32,
    );
}

impl<T: VideoBuffer> VideoFrameBufferExt for T {
    fn to_i420(&self) -> I420Buffer {
        self.to_i420()
    }

    fn to_argb(
        &self,
        format: VideoFormatType,
        dst: &mut [u8],
        dst_stride: u32,
        dst_width: i32,
        dst_height: i32,
    ) {
        self.to_argb(format, dst, dst_stride, dst_width, dst_height)
    }
}

impl_thread_safety!(I420Buffer, Send + Sync);
impl_thread_safety!(I420ABuffer, Send + Sync);
impl_thread_safety!(I422Buffer, Send + Sync);
impl_thread_safety!(I444Buffer, Send + Sync);
impl_thread_safety!(I010Buffer, Send + Sync);
impl_thread_safety!(NV12Buffer, Send + Sync);

#[cfg(target_arch = "wasm32")]
pub mod web {
    use super::VideoFrameBuffer;

    #[derive(Debug)]
    pub struct WebGlBuffer {}

    impl VideoFrameBuffer for WebGlBuffer {}
}

#[cfg(test)]
mod tests {
    use crate::video_frame::internal::BufferSealed;

    #[tokio::test]
    async fn video_frame_convert_test() {
        let mut i420 = super::I420Buffer::new(640, 480);

        // fill black data to i420
        {
            let (data_y, data_u, data_v) = i420.data_mut();
            for i in 0..data_y.len() {
                data_y[i] = 16;
            }
            for i in 0..data_u.len() {
                data_u[i] = 128;
            }
            for i in 0..data_v.len() {
                data_v[i] = 128;
            }
        }

        assert_eq!(i420.buffer().width(), 640);
        assert_eq!(i420.buffer().height(), 480);

        let scaled = i420.scale(320, 240);

        assert_eq!(scaled.buffer().width(), 320);
        assert_eq!(scaled.buffer().height(), 240);

        // check data in scaled
        {
            let (data_y, data_u, data_v) = scaled.data();
            for i in 0..data_y.len() {
                assert_eq!(data_y[i], 16);
            }
            for i in 0..data_u.len() {
                assert_eq!(data_u[i], 128);
            }
            for i in 0..data_v.len() {
                assert_eq!(data_v[i], 128);
            }
        }

        let mut rgba = vec![0u8; (320 * 240 * 4) as usize];
        i420.to_argb(super::VideoFormatType::ARGB, &mut rgba, 320 * 4, 320, 240);

        // check rgba data
        for i in 0..(320 * 240) as usize {
            let r = rgba[i * 4];
            let g = rgba[i * 4 + 1];
            let b = rgba[i * 4 + 2];
            let a = rgba[i * 4 + 3];
            assert_eq!(r, 0);
            assert_eq!(g, 0);
            assert_eq!(b, 0);
            assert_eq!(a, 255);
        }

        // fill green data to i420
        {
            let (data_y, data_u, data_v) = i420.data_mut();
            for i in 0..data_y.len() {
                data_y[i] = 145;
            }
            for i in 0..data_u.len() {
                data_u[i] = 54;
            }
            for i in 0..data_v.len() {
                data_v[i] = 34;
            }
        }

        i420.to_argb(super::VideoFormatType::ARGB, &mut rgba, 320 * 4, 320, 240);

        // check rgba data
        for i in 0..(320 * 240) as usize {
            //let r = rgba[i * 4];
            let g = rgba[i * 4 + 1];
            //let b = rgba[i * 4 + 2];
            let a = rgba[i * 4 + 3];
            assert_eq!(g, 255);
            assert_eq!(a, 255);
        }

        let i444 = i420.buffer().as_i444();
        assert!(i444.is_none());
    }
}
