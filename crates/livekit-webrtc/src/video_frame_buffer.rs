use cxx::UniquePtr;
use libwebrtc_sys::video_frame_buffer as vfb_sys;

pub use vfb_sys::ffi::VideoFrameBufferType;

pub struct VideoFrameBuffer {
    cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>,
}

impl VideoFrameBuffer {
    pub fn new(cxx_handle: UniquePtr<vfb_sys::ffi::VideoFrameBuffer>) -> Self {
        Self { cxx_handle }
    }

    pub fn buffer_type(&self) -> VideoFrameBufferType {
        self.cxx_handle.buffer_type()
    }
}
