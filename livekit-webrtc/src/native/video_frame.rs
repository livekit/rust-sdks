use crate::video_frame_buffer::VideoFrameBuffer;
use cxx::UniquePtr;
use std::fmt::{Debug, Formatter};
use webrtc_sys::video_frame as vf_sys;

#[derive(Debug)]
pub enum VideoRotation {
    VideoRotation0 = 0,
    VideoRotation90 = 90,
    VideoRotation180 = 180,
    VideoRotation270 = 270,
}

impl From<vf_sys::ffi::VideoRotation> for VideoRotation {
    fn from(rotation: vf_sys::ffi::VideoRotation) -> Self {
        match rotation {
            vf_sys::ffi::VideoRotation::VideoRotation0 => Self::VideoRotation0,
            vf_sys::ffi::VideoRotation::VideoRotation90 => Self::VideoRotation90,
            vf_sys::ffi::VideoRotation::VideoRotation180 => Self::VideoRotation180,
            vf_sys::ffi::VideoRotation::VideoRotation270 => Self::VideoRotation270,
            _ => unreachable!(),
        }
    }
}

impl From<VideoRotation> for vf_sys::ffi::VideoRotation {
    fn from(rotation: VideoRotation) -> Self {
        match rotation {
            VideoRotation::VideoRotation0 => Self::VideoRotation0,
            VideoRotation::VideoRotation90 => Self::VideoRotation90,
            VideoRotation::VideoRotation180 => Self::VideoRotation180,
            VideoRotation::VideoRotation270 => Self::VideoRotation270,
        }
    }
}

pub struct VideoFrame {
    cxx_handle: UniquePtr<vf_sys::ffi::VideoFrame>,
}

impl Debug for VideoFrame {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("VideoFrame")
            .field("width", &self.width())
            .field("height", &self.height())
            .field("id", &self.id())
            .field("rotation", &self.rotation())
            .field("timestamp", &self.timestamp())
            .finish()
    }
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
        self.cxx_handle.rotation().into()
    }

    /// # Safety
    /// Must be called only once, this function create the safe Rust
    /// wrapper around a VideoFrameBuffer.
    /// Only one wrapper musts exist at a time.
    pub(crate) unsafe fn video_frame_buffer(&self) -> VideoFrameBuffer {
        VideoFrameBuffer::new(self.cxx_handle.video_frame_buffer())
    }

    pub fn builder() -> VideoFrameBuilder {
        VideoFrameBuilder::default()
    }
}

pub struct VideoFrameBuilder {
    cxx_handle: UniquePtr<vf_sys::ffi::VideoFrameBuilder>,
}

impl Debug for VideoFrameBuilder {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("VideoFrameBuilder").finish()
    }
}

impl Default for VideoFrameBuilder {
    fn default() -> Self {
        Self {
            cxx_handle: vf_sys::ffi::create_video_frame_builder(),
        }
    }
}

impl VideoFrameBuilder {
    pub fn set_video_frame_buffer(mut self, buffer: VideoFrameBuffer) -> Self {
        self.cxx_handle
            .pin_mut()
            .set_video_frame_buffer(buffer.release());
        self
    }

    pub fn set_timestamp_us(mut self, ts_us: i64) -> Self {
        self.cxx_handle.pin_mut().set_timestamp_us(ts_us);
        self
    }

    pub fn set_rotation(mut self, rotation: VideoRotation) -> Self {
        self.cxx_handle.pin_mut().set_rotation(rotation.into());
        self
    }

    pub fn set_id(mut self, id: u16) -> Self {
        self.cxx_handle.pin_mut().set_id(id);
        self
    }

    pub fn build(mut self) -> VideoFrame {
        VideoFrame::new(self.cxx_handle.pin_mut().build())
    }
}
