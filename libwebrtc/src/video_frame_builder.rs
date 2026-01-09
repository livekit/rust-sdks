use crate::sys;

use crate::video_frame::{VideoRotation};
use crate::video_frame_buffer::{NativeVideoFrame};

pub struct VideoFrameBuilder {
    pub ffi: sys::RefCounted<sys::lkRefCountedObject>,
}

pub fn new_video_frame_builder() -> VideoFrameBuilder {
    let ffi = unsafe { sys::lkCreateVideoFrameBuilder() };
    VideoFrameBuilder { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
}

impl VideoFrameBuilder {
    pub fn set_video_frame_buffer_ptr(&mut self, ffi: sys::RefCounted<sys::lkRefCountedObject>) {
        unsafe {
            sys::lkVideoFrameBuilderSetVideoFrameBuffer(
                self.ffi.as_ptr(),
                ffi.as_ptr(),
            );
        }
    }

    pub fn set_rotation(&mut self, rotation: VideoRotation) {
        unsafe {
            sys::lkVideoFrameBuilderSetRotation(self.ffi.as_ptr(), rotation.into());
        }
    }

    pub fn set_timestamp_us(&mut self, timestamp_us: i64) {
        unsafe {
            sys::lkVideoFrameBuilderSetTimestampUs(self.ffi.as_ptr(), timestamp_us);
        }
    }

    pub fn set_id(&mut self, id: u16) {
        unsafe {
            sys::lkVideoFrameBuilderSetId(self.ffi.as_ptr(), id);
        }
    }

    pub fn build(&self) -> NativeVideoFrame {
        let ffi = unsafe { sys::lkVideoFrameBuilderBuild(self.ffi.as_ptr()) };
        NativeVideoFrame { ffi: unsafe { sys::RefCounted::from_raw(ffi) } }
    }
}
