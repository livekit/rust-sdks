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

        /// Wrap a Linux DMABUF file descriptor as a `kNative`
        /// `VideoFrameBuffer`. On non-Linux platforms this returns null.
        /// The fd is `dup()`'d internally; the caller retains ownership of
        /// the original.
        ///
        /// `colorspace_v4l2` is a V4L2 `v4l2_colorspace` value (e.g.
        /// `V4L2_COLORSPACE_REC709 == 3`,
        /// `V4L2_COLORSPACE_SMPTE170M == 1`); pass `0` to leave the value
        /// unspecified and let the encoder fall back to its default.
        fn new_native_buffer_from_dmabuf(
            dmabuf_fd: i32,
            fourcc: u32,
            width: i32,
            height: i32,
            total_size: u64,
            plane_offsets: &[u64],
            plane_strides: &[i32],
            colorspace_v4l2: u32,
        ) -> UniquePtr<VideoFrameBuffer>;

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
}

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

#[cfg(test)]
mod tests {
    use super::ffi;

    /// On non-Linux platforms `new_native_buffer_from_dmabuf` must return
    /// a null `UniquePtr` (the DMABUF feature is Linux-only). The test
    /// also runs on Linux where it still passes because the synthetic fd
    /// `-1` is rejected by `DmabufVideoFrameBuffer::Wrap`.
    #[test]
    fn dmabuf_buffer_rejects_invalid_fd() {
        let buf =
            ffi::new_native_buffer_from_dmabuf(-1, 0x32315559, 16, 16, 384, &[0], &[16], 0);
        assert!(buf.is_null(), "expected null buffer for invalid fd");
    }

    /// On Linux, create a memfd-backed test surface, wrap it as a DMABUF
    /// `VideoFrameBuffer`, and verify `ToI420()` produces pixel-identical
    /// output. `DMA_BUF_IOCTL_SYNC` is best-effort and harmlessly fails on
    /// memfds (logged once); mmap+memcpy still works.
    #[cfg(target_os = "linux")]
    #[test]
    fn dmabuf_buffer_to_i420_roundtrip_yuv420() {
        use std::os::raw::c_int;
        use std::os::unix::io::{FromRawFd, OwnedFd};

        let width: u32 = 16;
        let height: u32 = 16;
        let chroma_w = width as usize / 2;
        let chroma_h = height as usize / 2;
        let y_size = width as usize * height as usize;
        let uv_size = chroma_w * chroma_h;
        let total = y_size + 2 * uv_size;

        // memfd_create gives us an mmap'able fd without root or special
        // kernel modules. DMA_BUF_IOCTL_SYNC will harmlessly fail (warned
        // once); the mmap+memcpy path in ToI420 still works.
        let name = std::ffi::CString::new("dmabuf-test").unwrap();
        let fd: c_int =
            unsafe { libc::syscall(libc::SYS_memfd_create, name.as_ptr(), 0u32) as c_int };
        if fd < 0 {
            eprintln!("memfd_create unavailable; skipping DMABUF round-trip test");
            return;
        }
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };

        assert!(
            unsafe { libc::ftruncate(fd, total as libc::off_t) } == 0,
            "ftruncate: {}",
            std::io::Error::last_os_error()
        );
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                total,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        assert!(ptr != libc::MAP_FAILED);
        let slice = unsafe { std::slice::from_raw_parts_mut(ptr as *mut u8, total) };
        for row in 0..height as usize {
            for col in 0..width as usize {
                slice[row * width as usize + col] = ((row * 7 + col) & 0xFF) as u8;
            }
        }
        for i in 0..uv_size {
            slice[y_size + i] = 0x55;
            slice[y_size + uv_size + i] = 0xAA;
        }
        unsafe { libc::munmap(ptr, total) };

        let plane_offsets: [u64; 3] = [0, y_size as u64, (y_size + uv_size) as u64];
        let plane_strides: [i32; 3] = [width as i32, chroma_w as i32, chroma_w as i32];
        let buf = ffi::new_native_buffer_from_dmabuf(
            fd,
            0x32315559, // V4L2_PIX_FMT_YUV420 ("YU12")
            width as i32,
            height as i32,
            total as u64,
            &plane_offsets,
            &plane_strides,
            0,
        );
        // The wrap dup's the fd, so we can drop our copy now.
        drop(owned);
        assert!(!buf.is_null(), "DMABUF wrap should succeed for memfd");
        assert_eq!(buf.width(), width);
        assert_eq!(buf.height(), height);
        assert_eq!(buf.buffer_type(), ffi::VideoFrameBufferType::Native);

        let i420 = unsafe { buf.to_i420() };
        assert!(!i420.is_null());
        let yuv = unsafe { ffi::i420_to_yuv8(&*i420) };
        let stride_y = unsafe { (*ffi::yuv8_to_yuv(yuv)).stride_y() };
        let data_y = unsafe { (*yuv).data_y() };
        let first_row = unsafe { std::slice::from_raw_parts(data_y, stride_y as usize) };
        for col in 0..width as usize {
            assert_eq!(first_row[col], col as u8, "Y plane mismatch at col {col}");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn dmabuf_buffer_to_i420_respects_padded_yuv420_strides() {
        use std::os::raw::c_int;
        use std::os::unix::io::{FromRawFd, OwnedFd};

        let width: usize = 10;
        let height: usize = 6;
        let chroma_w = width / 2;
        let chroma_h = height / 2;
        let stride_y: usize = 16;
        let stride_uv: usize = 8;
        let y_size = stride_y * height;
        let u_size = stride_uv * chroma_h;
        let total = y_size + 2 * u_size;

        let name = std::ffi::CString::new("dmabuf-yuv420-stride-test").unwrap();
        let fd: c_int =
            unsafe { libc::syscall(libc::SYS_memfd_create, name.as_ptr(), 0u32) as c_int };
        if fd < 0 {
            eprintln!("memfd_create unavailable; skipping DMABUF stride test");
            return;
        }
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        assert!(
            unsafe { libc::ftruncate(fd, total as libc::off_t) } == 0,
            "ftruncate: {}",
            std::io::Error::last_os_error()
        );
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                total,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        assert!(ptr != libc::MAP_FAILED);
        let slice = unsafe { std::slice::from_raw_parts_mut(ptr as *mut u8, total) };
        slice.fill(0xEE);
        for row in 0..height {
            for col in 0..width {
                slice[row * stride_y + col] = (row * 11 + col) as u8;
            }
        }
        for row in 0..chroma_h {
            for col in 0..chroma_w {
                slice[y_size + row * stride_uv + col] = (0x40 + row * 7 + col) as u8;
                slice[y_size + u_size + row * stride_uv + col] = (0x90 + row * 5 + col) as u8;
            }
        }
        unsafe { libc::munmap(ptr, total) };

        let plane_offsets: [u64; 3] = [0, y_size as u64, (y_size + u_size) as u64];
        let plane_strides: [i32; 3] = [stride_y as i32, stride_uv as i32, stride_uv as i32];
        let buf = ffi::new_native_buffer_from_dmabuf(
            fd,
            0x32315559,
            width as i32,
            height as i32,
            total as u64,
            &plane_offsets,
            &plane_strides,
            0,
        );
        drop(owned);
        assert!(!buf.is_null(), "DMABUF wrap should succeed for padded YUV420");

        let i420 = unsafe { buf.to_i420() };
        assert!(!i420.is_null());
        let yuv = unsafe { ffi::i420_to_yuv8(&*i420) };
        let yuv = unsafe { &*ffi::yuv8_to_yuv(yuv) };
        let data_y = unsafe {
            std::slice::from_raw_parts((*yuv).data_y(), yuv.stride_y() as usize * height)
        };
        let data_u = unsafe {
            std::slice::from_raw_parts((*yuv).data_u(), yuv.stride_u() as usize * chroma_h)
        };
        let data_v = unsafe {
            std::slice::from_raw_parts((*yuv).data_v(), yuv.stride_v() as usize * chroma_h)
        };
        for row in 0..height {
            for col in 0..width {
                assert_eq!(data_y[row * yuv.stride_y() as usize + col], (row * 11 + col) as u8);
            }
        }
        for row in 0..chroma_h {
            for col in 0..chroma_w {
                assert_eq!(
                    data_u[row * yuv.stride_u() as usize + col],
                    (0x40 + row * 7 + col) as u8
                );
                assert_eq!(
                    data_v[row * yuv.stride_v() as usize + col],
                    (0x90 + row * 5 + col) as u8
                );
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn dmabuf_buffer_to_i420_accepts_single_plane_padded_yuv420() {
        use std::os::raw::c_int;
        use std::os::unix::io::{FromRawFd, OwnedFd};

        let width: usize = 10;
        let height: usize = 5;
        let chroma_w = width / 2;
        let chroma_h = (height + 1) / 2;
        let stride_y: usize = 16;
        let stride_uv: usize = 8;
        let storage_luma_h: usize = 16;
        let storage_chroma_h = storage_luma_h / 2;
        let y_size = stride_y * storage_luma_h;
        let u_size = stride_uv * storage_chroma_h;
        let total = y_size + 2 * u_size;

        let name = std::ffi::CString::new("dmabuf-yuv420-single-plane-test").unwrap();
        let fd: c_int =
            unsafe { libc::syscall(libc::SYS_memfd_create, name.as_ptr(), 0u32) as c_int };
        if fd < 0 {
            eprintln!("memfd_create unavailable; skipping single-plane DMABUF test");
            return;
        }
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        assert!(
            unsafe { libc::ftruncate(fd, total as libc::off_t) } == 0,
            "ftruncate: {}",
            std::io::Error::last_os_error()
        );
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                total,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        assert!(ptr != libc::MAP_FAILED);
        let slice = unsafe { std::slice::from_raw_parts_mut(ptr as *mut u8, total) };
        slice.fill(0xEE);
        for row in 0..height {
            for col in 0..width {
                slice[row * stride_y + col] = (row * 17 + col) as u8;
            }
        }
        for row in 0..chroma_h {
            for col in 0..chroma_w {
                slice[y_size + row * stride_uv + col] = (0x20 + row * 3 + col) as u8;
                slice[y_size + u_size + row * stride_uv + col] = (0xB0 + row * 5 + col) as u8;
            }
        }
        unsafe { libc::munmap(ptr, total) };

        let plane_offsets: [u64; 1] = [0];
        let plane_strides: [i32; 1] = [stride_y as i32];
        let buf = ffi::new_native_buffer_from_dmabuf(
            fd,
            0x32315559,
            width as i32,
            height as i32,
            total as u64,
            &plane_offsets,
            &plane_strides,
            0,
        );
        drop(owned);
        assert!(!buf.is_null(), "DMABUF wrap should accept single-plane YUV420");

        let i420 = unsafe { buf.to_i420() };
        assert!(!i420.is_null());
        let yuv = unsafe { ffi::i420_to_yuv8(&*i420) };
        let yuv = unsafe { &*ffi::yuv8_to_yuv(yuv) };
        let data_y = unsafe {
            std::slice::from_raw_parts((*yuv).data_y(), yuv.stride_y() as usize * height)
        };
        let data_u = unsafe {
            std::slice::from_raw_parts((*yuv).data_u(), yuv.stride_u() as usize * chroma_h)
        };
        let data_v = unsafe {
            std::slice::from_raw_parts((*yuv).data_v(), yuv.stride_v() as usize * chroma_h)
        };
        for row in 0..height {
            for col in 0..width {
                assert_eq!(data_y[row * yuv.stride_y() as usize + col], (row * 17 + col) as u8);
            }
        }
        for row in 0..chroma_h {
            for col in 0..chroma_w {
                assert_eq!(
                    data_u[row * yuv.stride_u() as usize + col],
                    (0x20 + row * 3 + col) as u8
                );
                assert_eq!(
                    data_v[row * yuv.stride_v() as usize + col],
                    (0xB0 + row * 5 + col) as u8
                );
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn dmabuf_buffer_to_i420_respects_padded_nv12_stride() {
        use std::os::raw::c_int;
        use std::os::unix::io::{FromRawFd, OwnedFd};

        let width: usize = 10;
        let height: usize = 6;
        let chroma_w = width / 2;
        let chroma_h = height / 2;
        let stride: usize = 16;
        let y_size = stride * height;
        let uv_size = stride * chroma_h;
        let total = y_size + uv_size;

        let name = std::ffi::CString::new("dmabuf-nv12-stride-test").unwrap();
        let fd: c_int =
            unsafe { libc::syscall(libc::SYS_memfd_create, name.as_ptr(), 0u32) as c_int };
        if fd < 0 {
            eprintln!("memfd_create unavailable; skipping DMABUF NV12 stride test");
            return;
        }
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        assert!(
            unsafe { libc::ftruncate(fd, total as libc::off_t) } == 0,
            "ftruncate: {}",
            std::io::Error::last_os_error()
        );
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                total,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        assert!(ptr != libc::MAP_FAILED);
        let slice = unsafe { std::slice::from_raw_parts_mut(ptr as *mut u8, total) };
        slice.fill(0xEE);
        for row in 0..height {
            for col in 0..width {
                slice[row * stride + col] = (row * 13 + col) as u8;
            }
        }
        for row in 0..chroma_h {
            for col in 0..chroma_w {
                let uv = y_size + row * stride + col * 2;
                slice[uv] = (0x30 + row * 3 + col) as u8;
                slice[uv + 1] = (0xA0 + row * 9 + col) as u8;
            }
        }
        unsafe { libc::munmap(ptr, total) };

        let plane_offsets: [u64; 2] = [0, y_size as u64];
        let plane_strides: [i32; 2] = [stride as i32, stride as i32];
        let buf = ffi::new_native_buffer_from_dmabuf(
            fd,
            0x3231564E,
            width as i32,
            height as i32,
            total as u64,
            &plane_offsets,
            &plane_strides,
            0,
        );
        drop(owned);
        assert!(!buf.is_null(), "DMABUF wrap should succeed for padded NV12");

        let i420 = unsafe { buf.to_i420() };
        assert!(!i420.is_null());
        let yuv = unsafe { ffi::i420_to_yuv8(&*i420) };
        let yuv = unsafe { &*ffi::yuv8_to_yuv(yuv) };
        let data_y = unsafe {
            std::slice::from_raw_parts((*yuv).data_y(), yuv.stride_y() as usize * height)
        };
        let data_u = unsafe {
            std::slice::from_raw_parts((*yuv).data_u(), yuv.stride_u() as usize * chroma_h)
        };
        let data_v = unsafe {
            std::slice::from_raw_parts((*yuv).data_v(), yuv.stride_v() as usize * chroma_h)
        };
        for row in 0..height {
            for col in 0..width {
                assert_eq!(data_y[row * yuv.stride_y() as usize + col], (row * 13 + col) as u8);
            }
        }
        for row in 0..chroma_h {
            for col in 0..chroma_w {
                assert_eq!(
                    data_u[row * yuv.stride_u() as usize + col],
                    (0x30 + row * 3 + col) as u8
                );
                assert_eq!(
                    data_v[row * yuv.stride_v() as usize + col],
                    (0xA0 + row * 9 + col) as u8
                );
            }
        }
    }
}
