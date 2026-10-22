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

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    #[derive(Debug)]
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

    unsafe extern "C++" {
        include!("livekit/video_frame_buffer.h");

        type VideoFrameBuffer;
        type PlanarYuvBuffer;
        type PlanarYuv8Buffer;
        type PlanarYuv16BBuffer;
        type BiplanarYuvBuffer;
        type BiplanarYuv8Buffer;
        type I420Buffer;
        type I420ABuffer;
        type I422Buffer;
        type I444Buffer;
        type I010Buffer;
        type NV12Buffer;
        type PlatformImageBuffer;
        type D3D11Texture2D;

        fn buffer_type(self: &VideoFrameBuffer) -> VideoFrameBufferType;
        fn width(self: &VideoFrameBuffer) -> u32;
        fn height(self: &VideoFrameBuffer) -> u32;

        /// # SAFETY
        /// If the buffer type is I420, the buffer must be cloned before
        unsafe fn to_i420(self: &VideoFrameBuffer) -> UniquePtr<I420Buffer>;

        /// # SAFETY
        /// The functions require ownership
        unsafe fn get_i420(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<I420Buffer>;
        unsafe fn get_i420a(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<I420ABuffer>;
        unsafe fn get_i422(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<I422Buffer>;
        unsafe fn get_i444(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<I444Buffer>;
        unsafe fn get_i010(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<I010Buffer>;
        unsafe fn get_nv12(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<NV12Buffer>;

        fn chroma_width(self: &PlanarYuvBuffer) -> u32;
        fn chroma_height(self: &PlanarYuvBuffer) -> u32;
        fn stride_y(self: &PlanarYuvBuffer) -> u32;
        fn stride_u(self: &PlanarYuvBuffer) -> u32;
        fn stride_v(self: &PlanarYuvBuffer) -> u32;

        fn data_y(self: &PlanarYuv8Buffer) -> *const u8;
        fn data_u(self: &PlanarYuv8Buffer) -> *const u8;
        fn data_v(self: &PlanarYuv8Buffer) -> *const u8;

        fn data_y(self: &PlanarYuv16BBuffer) -> *const u16;
        fn data_u(self: &PlanarYuv16BBuffer) -> *const u16;
        fn data_v(self: &PlanarYuv16BBuffer) -> *const u16;

        fn chroma_width(self: &BiplanarYuvBuffer) -> u32;
        fn chroma_height(self: &BiplanarYuvBuffer) -> u32;
        fn stride_y(self: &BiplanarYuvBuffer) -> u32;
        fn stride_uv(self: &BiplanarYuvBuffer) -> u32;

        fn data_y(self: &BiplanarYuv8Buffer) -> *const u8;
        fn data_uv(self: &BiplanarYuv8Buffer) -> *const u8;

        fn stride_a(self: &I420ABuffer) -> u32;
        fn data_a(self: &I420ABuffer) -> *const u8;

        fn scale(self: &I420Buffer, scaled_width: i32, scaled_height: i32)
            -> UniquePtr<I420Buffer>;
        fn scale(
            self: &I420ABuffer,
            scaled_width: i32,
            scaled_height: i32,
        ) -> UniquePtr<I420ABuffer>;
        fn scale(self: &I422Buffer, scaled_width: i32, scaled_height: i32)
            -> UniquePtr<I422Buffer>;
        fn scale(self: &I444Buffer, scaled_width: i32, scaled_height: i32)
            -> UniquePtr<I444Buffer>;
        fn scale(self: &I010Buffer, scaled_width: i32, scaled_height: i32)
            -> UniquePtr<I010Buffer>;
        fn scale(self: &NV12Buffer, scaled_width: i32, scaled_height: i32)
            -> UniquePtr<NV12Buffer>;

        fn copy_i420_buffer(i420: &UniquePtr<I420Buffer>) -> UniquePtr<I420Buffer>;
        fn new_i420_buffer(
            width: i32,
            height: i32,
            stride_y: i32,
            stride_u: i32,
            stride_v: i32,
        ) -> UniquePtr<I420Buffer>;

        /// Like `new_i420_buffer`, but with the pixel data set to black
        /// (Y=0, U=V=128) instead of left uninitialized.
        fn new_black_i420_buffer(
            width: i32,
            height: i32,
            stride_y: i32,
            stride_u: i32,
            stride_v: i32,
        ) -> UniquePtr<I420Buffer>;

        fn new_i422_buffer(
            width: i32,
            height: i32,
            stride_y: i32,
            stride_u: i32,
            stride_v: i32,
        ) -> UniquePtr<I422Buffer>;

        fn new_i444_buffer(
            width: i32,
            height: i32,
            stride_y: i32,
            stride_u: i32,
            stride_v: i32,
        ) -> UniquePtr<I444Buffer>;

        fn new_i010_buffer(
            width: i32,
            height: i32,
            stride_y: i32,
            stride_u: i32,
            stride_v: i32,
        ) -> UniquePtr<I010Buffer>;

        fn new_nv12_buffer(
            width: i32,
            height: i32,
            stride_y: i32,
            stride_uv: i32,
        ) -> UniquePtr<NV12Buffer>;

        unsafe fn new_native_buffer_from_platform_image_buffer(
            platform_native_buffer: *mut PlatformImageBuffer,
        ) -> UniquePtr<VideoFrameBuffer>;
        unsafe fn native_buffer_to_platform_image_buffer(
            buffer: &UniquePtr<VideoFrameBuffer>,
        ) -> *mut PlatformImageBuffer;

        /// Creates a native buffer backed by a DMA-BUF file descriptor
        /// (Jetson `NvBufSurface`). `pixel_format` is a
        /// `livekit::DmaBufPixelFormat` value: `0` for NV12, `1` for YUV420M.
        ///
        /// The buffer does not take ownership of the fd: it must remain valid
        /// until every frame captured from it has been consumed downstream.
        fn new_native_buffer_from_dmabuf(
            dmabuf_fd: i32,
            width: i32,
            height: i32,
            pixel_format: i32,
        ) -> UniquePtr<VideoFrameBuffer>;

        /// Returns the DMA-BUF fd backing this buffer, or `-1` if this buffer
        /// is not DMA-BUF backed.
        fn native_buffer_to_dmabuf_fd(buffer: &UniquePtr<VideoFrameBuffer>) -> i32;

        /// Creates a native buffer on the top-left `width` x `height` of a
        /// BGRA `ID3D11Texture2D` (`livekit::D3D11VideoFrameBuffer`), taking a
        /// reference on the texture. `keep_alive` is dropped when libwebrtc
        /// releases the last view of the frame. Null for an unsupported
        /// texture, and always null outside Windows.
        ///
        /// # SAFETY
        /// `texture` must be null or a valid `ID3D11Texture2D` whose device
        /// has multithread protection on.
        unsafe fn new_native_buffer_from_d3d11_texture(
            texture: *mut D3D11Texture2D,
            width: i32,
            height: i32,
            keep_alive: Box<D3D11KeepAlive>,
        ) -> UniquePtr<VideoFrameBuffer>;

        /// The texture behind a D3D11 native buffer (no reference added), or
        /// null if this buffer is not one.
        fn native_buffer_to_d3d11_texture(
            buffer: &UniquePtr<VideoFrameBuffer>,
        ) -> *mut D3D11Texture2D;

        /// GPU-to-CPU readbacks done by D3D11 native buffers in this process.
        fn d3d11_readback_count() -> u64;

        unsafe fn yuv_to_vfb(yuv: *const PlanarYuvBuffer) -> *const VideoFrameBuffer;
        unsafe fn biyuv_to_vfb(yuv: *const BiplanarYuvBuffer) -> *const VideoFrameBuffer;
        unsafe fn yuv8_to_yuv(yuv8: *const PlanarYuv8Buffer) -> *const PlanarYuvBuffer;
        unsafe fn yuv16b_to_yuv(yuv16b: *const PlanarYuv16BBuffer) -> *const PlanarYuvBuffer;
        unsafe fn biyuv8_to_biyuv(biyuv8: *const BiplanarYuv8Buffer) -> *const BiplanarYuvBuffer;
        unsafe fn i420_to_yuv8(i420: *const I420Buffer) -> *const PlanarYuv8Buffer;
        unsafe fn i420a_to_yuv8(i420a: *const I420ABuffer) -> *const PlanarYuv8Buffer;
        unsafe fn i422_to_yuv8(i422: *const I422Buffer) -> *const PlanarYuv8Buffer;
        unsafe fn i444_to_yuv8(i444: *const I444Buffer) -> *const PlanarYuv8Buffer;
        unsafe fn i010_to_yuv16b(i010: *const I010Buffer) -> *const PlanarYuv16BBuffer;
        unsafe fn nv12_to_biyuv8(nv12: *const NV12Buffer) -> *const BiplanarYuv8Buffer;

        fn _unique_video_frame_buffer() -> UniquePtr<VideoFrameBuffer>;
    }

    extern "Rust" {
        type D3D11KeepAlive;
    }
}

/// Whatever keeps a D3D11 frame's texture from being overwritten while
/// libwebrtc still reads it (a capture pool's slot, say). C++ drops it with
/// the last view of the frame, on whichever thread releases that view.
pub struct D3D11KeepAlive(pub Box<dyn std::any::Any + Send>);

impl_thread_safety!(ffi::VideoFrameBuffer, Send + Sync);
impl_thread_safety!(ffi::PlanarYuvBuffer, Send + Sync);
impl_thread_safety!(ffi::PlanarYuv8Buffer, Send + Sync);
impl_thread_safety!(ffi::PlanarYuv16BBuffer, Send + Sync);
impl_thread_safety!(ffi::BiplanarYuvBuffer, Send + Sync);
impl_thread_safety!(ffi::BiplanarYuv8Buffer, Send + Sync);
impl_thread_safety!(ffi::I420Buffer, Send + Sync);
impl_thread_safety!(ffi::I420ABuffer, Send + Sync);
impl_thread_safety!(ffi::I422Buffer, Send + Sync);
impl_thread_safety!(ffi::I444Buffer, Send + Sync);
impl_thread_safety!(ffi::I010Buffer, Send + Sync);
impl_thread_safety!(ffi::NV12Buffer, Send + Sync);
