use cxx::UniquePtr;
use libwebrtc_sys::video_frame as vf_sys;

pub use vf_sys::ffi::VideoRotation;

use crate::video_frame_buffer::VideoFrameBuffer;

pub struct VideoFrame {
    cxx_handle: UniquePtr<vf_sys::ffi::VideoFrame>,
}

impl VideoFrame {
    pub(crate) fn new(cxx_handle: UniquePtr<vf_sys::ffi::VideoFrame>) -> Self {
        Self { cxx_handle }
    }

    pub fn width(&self) -> i32 {
        self.cxx_handle.width()
    }

    pub fn height(&self) -> i32 {
        self.cxx_handle.height()
    }

    pub fn size(&self) -> u32 {
        self.cxx_handle.size()
    }

    pub fn id(&self) -> u16 {
        self.cxx_handle.id()
    }

    pub fn timestamp_us(&self) -> i64 {
        self.cxx_handle.timestamp_us()
    }

    pub fn ntp_time_ms(&self) -> i64 {
        self.cxx_handle.ntp_time_ms()
    }

    pub fn transport_frame_id(&self) -> u32 {
        self.cxx_handle.transport_frame_id()
    }

    pub fn timestamp(&self) -> u32 {
        self.cxx_handle.timestamp()
    }

    pub fn rotation(&self) -> VideoRotation {
        self.cxx_handle.rotation()
    }

    /// # Safety
    /// Must be called only once, this function create the safe Rust
    /// wrapper around a VideoFrameBuffer.
    /// Only one wrapper musts exist at a time.
    pub(crate) unsafe fn video_frame_buffer(&self) -> VideoFrameBuffer {
        VideoFrameBuffer::new(self.cxx_handle.video_frame_buffer())
    }
}
