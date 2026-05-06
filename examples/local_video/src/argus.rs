//! Thin FFI wrapper around NVIDIA Argus/libargus for MIPI CSI camera capture.
//!
//! This module provides DMA-buffer frame acquisition from MIPI cameras on Jetson
//! platforms. Frames are blitted from Argus' EGLStream frame into NvBufSurface
//! DMA file descriptors that can be passed to the hardware encoder without
//! CPU-side pixel copies.
//!
//! The Argus API is C++, so we use a small C shim (linked via build.rs on
//! Jetson) to expose the capture session lifecycle.

use std::ffi::c_int;
use std::io;

/// Opaque handle to an Argus capture session.
pub struct ArgusCaptureSession {
    handle: *mut std::ffi::c_void,
    width: u32,
    height: u32,
}

/// A captured Argus frame backed by a DMA buffer.
pub struct ArgusFrame {
    /// DMA buffer file descriptor containing an NV12 frame.
    pub dmabuf_fd: i32,
    /// Argus sensor start timestamp in nanoseconds, when available.
    pub sensor_timestamp_ns: Option<u64>,
    /// Time spent waiting for `FrameConsumer::acquireFrame` to return.
    pub acquire_wait_ns: u64,
    /// Time spent copying the acquired EGLStream frame into the DMA buffer.
    pub blit_ns: u64,
}

// The C++ session is single-threaded but we move it across the tokio runtime.
unsafe impl Send for ArgusCaptureSession {}

extern "C" {
    fn lk_argus_create_session(
        sensor_index: c_int,
        width: c_int,
        height: c_int,
        fps: c_int,
    ) -> *mut std::ffi::c_void;

    fn lk_argus_destroy_session(session: *mut std::ffi::c_void);

    /// Acquire the next frame and optionally return the Argus sensor timestamp.
    /// Returns the NvBufSurface DMA fd, or -1 on error.
    /// The fd is valid until the next acquire call or `lk_argus_release_frame`.
    fn lk_argus_acquire_frame_with_metadata(
        session: *mut std::ffi::c_void,
        sensor_timestamp_ns: *mut u64,
        acquire_wait_ns: *mut u64,
        blit_ns: *mut u64,
    ) -> c_int;

    /// Release the most recently acquired frame back to the Argus buffer pool.
    fn lk_argus_release_frame(session: *mut std::ffi::c_void);
}

impl ArgusCaptureSession {
    /// Open an Argus capture session on the given MIPI CSI sensor.
    ///
    /// `sensor_index` selects the camera (0 for the first CSI camera).
    /// The session negotiates the given resolution and framerate with the ISP.
    pub fn new(sensor_index: u32, width: u32, height: u32, fps: u32) -> io::Result<Self> {
        let handle = unsafe {
            lk_argus_create_session(
                sensor_index as c_int,
                width as c_int,
                height as c_int,
                fps as c_int,
            )
        };
        if handle.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to create Argus capture session",
            ));
        }
        Ok(Self { handle, width, height })
    }

    /// Acquire the next captured frame as a DMA buffer.
    ///
    /// The returned fd refers to an NvBufSurface in NV12 format. It remains
    /// valid until [`release_frame`](Self::release_frame) is called or the
    /// next `acquire_frame` implicitly releases the previous one.
    pub fn acquire_frame(&mut self) -> io::Result<ArgusFrame> {
        let mut sensor_timestamp_ns = 0;
        let mut acquire_wait_ns = 0;
        let mut blit_ns = 0;
        let fd = unsafe {
            lk_argus_acquire_frame_with_metadata(
                self.handle,
                &mut sensor_timestamp_ns,
                &mut acquire_wait_ns,
                &mut blit_ns,
            )
        };
        if fd < 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "Argus frame acquisition failed"));
        }
        Ok(ArgusFrame {
            dmabuf_fd: fd,
            sensor_timestamp_ns: (sensor_timestamp_ns > 0).then_some(sensor_timestamp_ns),
            acquire_wait_ns,
            blit_ns,
        })
    }

    /// Release the most recently acquired frame back to the buffer pool.
    pub fn release_frame(&mut self) {
        unsafe { lk_argus_release_frame(self.handle) };
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

impl Drop for ArgusCaptureSession {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { lk_argus_destroy_session(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}
