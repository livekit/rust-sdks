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

use crate::imp::video_frame as vf_imp;

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

/// Metadata carried alongside a video frame via the packet trailer mechanism.
///
/// Each field corresponds to an independently negotiable packet trailer feature
/// (`PTF_USER_TIMESTAMP`, `PTF_FRAME_ID`, `PTF_USER_DATA`), so individual fields
/// are `Option`.
#[derive(Debug, Clone)]
pub struct FrameMetadata {
    /// Wall-clock capture time in microseconds, when `PTF_USER_TIMESTAMP` is enabled.
    pub user_timestamp: Option<u64>,
    /// Monotonically increasing frame identifier, when `PTF_FRAME_ID` is enabled.
    pub frame_id: Option<u32>,
    /// Arbitrary application-supplied bytes, when `PTF_USER_DATA` is enabled.
    ///
    /// Bounded by the packet trailer size budget (~232 bytes when the other
    /// features are also active); oversize payloads are dropped on the send
    /// side rather than truncated.
    pub user_data: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct VideoFrame<T>
where
    T: AsRef<dyn VideoBuffer>,
{
    pub rotation: VideoRotation,
    pub timestamp_us: i64, // When the frame was captured in microseconds
    /// Packet-trailer metadata, if any trailer features are active.
    pub frame_metadata: Option<FrameMetadata>,
    pub buffer: T,
}

impl<T: AsRef<dyn VideoBuffer>> VideoFrame<T> {
    pub fn new(rotation: VideoRotation, buffer: T) -> Self {
        Self { rotation, timestamp_us: 0, frame_metadata: None, buffer }
    }
}

pub type BoxVideoBuffer = Box<dyn VideoBuffer>;
pub type BoxVideoFrame = VideoFrame<BoxVideoBuffer>;

pub(crate) mod internal {
    use super::{I420Buffer, VideoFormatType};

    pub trait BufferSealed: Send + Sync {
        #[cfg(not(target_arch = "wasm32"))]
        fn sys_handle(&self) -> &webrtc_sys::video_frame_buffer::ffi::VideoFrameBuffer;

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

pub trait VideoBuffer: internal::BufferSealed + Debug {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn buffer_type(&self) -> VideoBufferType;

    #[cfg(not(target_arch = "wasm32"))]
    fn as_native(&self) -> Option<&native::NativeBuffer> {
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

macro_rules! new_buffer_type {
    ($type:ident, $variant:ident, $as:ident) => {
        pub struct $type {
            pub(crate) handle: vf_imp::$type,
        }

        impl $crate::video_frame::internal::BufferSealed for $type {
            #[cfg(not(target_arch = "wasm32"))]
            fn sys_handle(&self) -> &webrtc_sys::video_frame_buffer::ffi::VideoFrameBuffer {
                self.handle.sys_handle()
            }

            #[cfg(not(target_arch = "wasm32"))]
            fn to_i420(&self) -> I420Buffer {
                I420Buffer { handle: self.handle.to_i420() }
            }

            #[cfg(not(target_arch = "wasm32"))]
            fn to_argb(
                &self,
                format: VideoFormatType,
                dst: &mut [u8],
                stride: u32,
                width: i32,
                height: i32,
            ) {
                self.handle.to_argb(format, dst, stride, width, height)
            }
        }

        impl VideoBuffer for $type {
            fn width(&self) -> u32 {
                self.handle.width()
            }

            fn height(&self) -> u32 {
                self.handle.height()
            }

            fn buffer_type(&self) -> VideoBufferType {
                VideoBufferType::$variant
            }

            fn $as(&self) -> Option<&$type> {
                Some(self)
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

impl I420Buffer {
    pub fn with_strides(
        width: u32,
        height: u32,
        stride_y: u32,
        stride_u: u32,
        stride_v: u32,
    ) -> I420Buffer {
        vf_imp::I420Buffer::new(width, height, stride_y, stride_u, stride_v)
    }

    pub fn new(width: u32, height: u32) -> I420Buffer {
        Self::with_strides(width, height, width, (width + 1) / 2, (width + 1) / 2)
    }

    pub fn chroma_width(&self) -> u32 {
        self.handle.chroma_width()
    }

    pub fn chroma_height(&self) -> u32 {
        self.handle.chroma_height()
    }

    pub fn strides(&self) -> (u32, u32, u32) {
        (self.handle.stride_y(), self.handle.stride_u(), self.handle.stride_v())
    }

    pub fn data(&self) -> (&[u8], &[u8], &[u8]) {
        self.handle.data()
    }

    pub fn data_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        let (data_y, data_u, data_v) = self.handle.data();
        unsafe {
            (
                std::slice::from_raw_parts_mut(data_y.as_ptr() as *mut u8, data_y.len()),
                std::slice::from_raw_parts_mut(data_u.as_ptr() as *mut u8, data_u.len()),
                std::slice::from_raw_parts_mut(data_v.as_ptr() as *mut u8, data_v.len()),
            )
        }
    }

    pub fn scale(&mut self, scaled_width: i32, scaled_height: i32) -> I420Buffer {
        self.handle.scale(scaled_width, scaled_height)
    }
}

impl I420ABuffer {
    pub fn chroma_width(&self) -> u32 {
        self.handle.chroma_width()
    }

    pub fn chroma_height(&self) -> u32 {
        self.handle.chroma_height()
    }

    pub fn strides(&self) -> (u32, u32, u32, u32) {
        (
            self.handle.stride_y(),
            self.handle.stride_u(),
            self.handle.stride_v(),
            self.handle.stride_a(),
        )
    }

    #[allow(clippy::type_complexity)]
    pub fn data(&self) -> (&[u8], &[u8], &[u8], Option<&[u8]>) {
        self.handle.data()
    }

    #[allow(clippy::type_complexity)]
    pub fn data_mut(&self) -> (&mut [u8], &mut [u8], &mut [u8], Option<&mut [u8]>) {
        let (data_y, data_u, data_v, data_a) = self.handle.data();
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
        self.handle.scale(scaled_width, scaled_height)
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
        vf_imp::I422Buffer::new(width, height, stride_y, stride_u, stride_v)
    }

    pub fn new(width: u32, height: u32) -> I422Buffer {
        Self::with_strides(width, height, width, (width + 1) / 2, (width + 1) / 2)
    }

    pub fn chroma_width(&self) -> u32 {
        self.handle.chroma_width()
    }

    pub fn chroma_height(&self) -> u32 {
        self.handle.chroma_height()
    }

    pub fn strides(&self) -> (u32, u32, u32) {
        (self.handle.stride_y(), self.handle.stride_u(), self.handle.stride_v())
    }

    pub fn data(&self) -> (&[u8], &[u8], &[u8]) {
        self.handle.data()
    }

    pub fn data_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        let (data_y, data_u, data_v) = self.handle.data();
        unsafe {
            (
                std::slice::from_raw_parts_mut(data_y.as_ptr() as *mut u8, data_y.len()),
                std::slice::from_raw_parts_mut(data_u.as_ptr() as *mut u8, data_u.len()),
                std::slice::from_raw_parts_mut(data_v.as_ptr() as *mut u8, data_v.len()),
            )
        }
    }

    pub fn scale(&mut self, scaled_width: i32, scaled_height: i32) -> I422Buffer {
        self.handle.scale(scaled_width, scaled_height)
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
        vf_imp::I444Buffer::new(width, height, stride_y, stride_u, stride_v)
    }

    pub fn new(width: u32, height: u32) -> I444Buffer {
        Self::with_strides(width, height, width, width, width)
    }

    pub fn chroma_width(&self) -> u32 {
        self.handle.chroma_width()
    }

    pub fn chroma_height(&self) -> u32 {
        self.handle.chroma_height()
    }

    pub fn strides(&self) -> (u32, u32, u32) {
        (self.handle.stride_y(), self.handle.stride_u(), self.handle.stride_v())
    }

    pub fn data(&self) -> (&[u8], &[u8], &[u8]) {
        self.handle.data()
    }

    pub fn data_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        let (data_y, data_u, data_v) = self.handle.data();
        unsafe {
            (
                std::slice::from_raw_parts_mut(data_y.as_ptr() as *mut u8, data_y.len()),
                std::slice::from_raw_parts_mut(data_u.as_ptr() as *mut u8, data_u.len()),
                std::slice::from_raw_parts_mut(data_v.as_ptr() as *mut u8, data_v.len()),
            )
        }
    }

    pub fn scale(&mut self, scaled_width: i32, scaled_height: i32) -> I444Buffer {
        self.handle.scale(scaled_width, scaled_height)
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
        vf_imp::I010Buffer::new(width, height, stride_y, stride_u, stride_v)
    }

    pub fn new(width: u32, height: u32) -> I010Buffer {
        Self::with_strides(width, height, width, (width + 1) / 2, (width + 1) / 2)
    }

    pub fn chroma_width(&self) -> u32 {
        self.handle.chroma_width()
    }

    pub fn chroma_height(&self) -> u32 {
        self.handle.chroma_height()
    }

    pub fn strides(&self) -> (u32, u32, u32) {
        (self.handle.stride_y(), self.handle.stride_u(), self.handle.stride_v())
    }

    pub fn data(&self) -> (&[u16], &[u16], &[u16]) {
        self.handle.data()
    }

    pub fn data_mut(&mut self) -> (&mut [u16], &mut [u16], &mut [u16]) {
        let (data_y, data_u, data_v) = self.handle.data();
        unsafe {
            (
                std::slice::from_raw_parts_mut(data_y.as_ptr() as *mut u16, data_y.len()),
                std::slice::from_raw_parts_mut(data_u.as_ptr() as *mut u16, data_u.len()),
                std::slice::from_raw_parts_mut(data_v.as_ptr() as *mut u16, data_v.len()),
            )
        }
    }

    pub fn scale(&mut self, scaled_width: i32, scaled_height: i32) -> I010Buffer {
        self.handle.scale(scaled_width, scaled_height)
    }
}

impl NV12Buffer {
    pub fn with_strides(width: u32, height: u32, stride_y: u32, stride_uv: u32) -> NV12Buffer {
        vf_imp::NV12Buffer::new(width, height, stride_y, stride_uv)
    }

    pub fn new(width: u32, height: u32) -> NV12Buffer {
        Self::with_strides(width, height, width, width + width % 2)
    }

    pub fn chroma_width(&self) -> u32 {
        self.handle.chroma_width()
    }

    pub fn chroma_height(&self) -> u32 {
        self.handle.chroma_height()
    }

    pub fn strides(&self) -> (u32, u32) {
        (self.handle.stride_y(), self.handle.stride_uv())
    }

    pub fn data(&self) -> (&[u8], &[u8]) {
        self.handle.data()
    }

    pub fn data_mut(&mut self) -> (&mut [u8], &mut [u8]) {
        let (data_y, data_uv) = self.handle.data();
        unsafe {
            (
                std::slice::from_raw_parts_mut(data_y.as_ptr() as *mut u8, data_y.len()),
                std::slice::from_raw_parts_mut(data_uv.as_ptr() as *mut u8, data_uv.len()),
            )
        }
    }

    pub fn scale(&mut self, scaled_width: i32, scaled_height: i32) -> NV12Buffer {
        self.handle.scale(scaled_width, scaled_height)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::fmt::Debug;
    #[cfg(target_os = "linux")]
    use std::os::fd::OwnedFd;

    #[cfg(target_os = "linux")]
    use thiserror::Error;

    use super::{vf_imp, I420Buffer, VideoBuffer, VideoBufferType, VideoFormatType};

    new_buffer_type!(NativeBuffer, Native, as_native);

    /// A borrowed NVIDIA NVDEC frame stored as NV12 in CUDA device memory.
    #[cfg(target_os = "linux")]
    pub struct CudaNv12Frame<'a> {
        pub(crate) buffer: &'a vf_imp::NativeBuffer,
    }

    /// A CUDA import of a Vulkan external-memory staging buffer.
    #[cfg(target_os = "linux")]
    pub struct CudaNv12RenderTarget {
        pub(crate) handle: vf_imp::CudaNv12RenderTarget,
    }

    #[cfg(target_os = "linux")]
    impl std::fmt::Debug for CudaNv12RenderTarget {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.debug_struct("CudaNv12RenderTarget").finish_non_exhaustive()
        }
    }

    /// An error while importing or copying a CUDA NV12 render target.
    #[derive(Debug, Error)]
    #[cfg(target_os = "linux")]
    pub enum CudaInteropError {
        /// CUDA could not import the external Vulkan memory or semaphore.
        #[error("CUDA could not import the Vulkan render target")]
        ExternalImport,
        /// The frame could not be copied into the imported target.
        #[error("CUDA could not copy the NV12 frame into the render target")]
        Copy,
    }

    #[cfg(target_os = "linux")]
    impl CudaNv12Frame<'_> {
        /// Returns the visible frame width in pixels.
        pub fn width(&self) -> u32 {
            self.buffer.width()
        }

        /// Returns the visible frame height in pixels.
        pub fn height(&self) -> u32 {
            self.buffer.height()
        }

        /// Returns the CUDA allocation pitch shared by the luma and chroma planes.
        pub fn pitch(&self) -> u32 {
            self.buffer.cuda_nv12_stride()
        }

        /// Returns the UUID of the CUDA device which owns this frame.
        pub fn device_uuid(&self) -> [u8; 16] {
            self.buffer.cuda_nv12_device_uuid()
        }

        /// Imports an owned Vulkan buffer and semaphore file descriptor into CUDA.
        pub fn create_render_target(
            &self,
            memory_fd: OwnedFd,
            allocation_size: u64,
            destination_pitch: u32,
            uv_offset: u64,
            semaphore_fd: OwnedFd,
        ) -> Result<CudaNv12RenderTarget, CudaInteropError> {
            self.buffer
                .new_cuda_nv12_render_target(
                    memory_fd,
                    allocation_size,
                    destination_pitch,
                    uv_offset,
                    semaphore_fd,
                )
                .map(|handle| CudaNv12RenderTarget { handle })
                .ok_or(CudaInteropError::ExternalImport)
        }

        /// Copies this frame into an imported render target and signals its semaphore.
        pub fn copy_to(&self, target: &mut CudaNv12RenderTarget) -> Result<(), CudaInteropError> {
            self.buffer
                .copy_cuda_nv12_to(&mut target.handle)
                .then_some(())
                .ok_or(CudaInteropError::Copy)
        }
    }

    impl NativeBuffer {
        /// Creates a `NativeBuffer` from a `CVPixelBufferRef` pointer.
        ///
        /// This function does not bump the reference count of the pixel buffer.
        ///
        /// Safety: The given pointer must be a valid `CVPixelBufferRef`.
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        pub unsafe fn from_cv_pixel_buffer(cv_pixel_buffer: *mut std::ffi::c_void) -> Self {
            vf_imp::NativeBuffer::from_cv_pixel_buffer(cv_pixel_buffer)
        }

        /// Returns the `CVPixelBufferRef` that backs this buffer, or `null` if
        /// this buffer is not currently backed by a `CVPixelBufferRef`.
        ///
        /// This function does not bump the reference count of the pixel buffer.
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        pub fn get_cv_pixel_buffer(&self) -> *mut std::ffi::c_void {
            self.handle.get_cv_pixel_buffer()
        }

        /// Returns a CUDA NV12 view when this native frame came from NVIDIA NVDEC.
        #[cfg(target_os = "linux")]
        pub fn cuda_nv12(&self) -> Option<CudaNv12Frame<'_>> {
            self.handle.cuda_nv12()
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
}

#[cfg(target_arch = "wasm32")]
pub mod web {
    use super::VideoFrameBuffer;

    #[derive(Debug)]
    pub struct WebGlBuffer {}

    impl VideoFrameBuffer for WebGlBuffer {}
}
