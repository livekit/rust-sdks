use crate::video_frame::{VideoFrame, VideoFrameBuffer};
use cxx::SharedPtr;
use webrtc_sys::video_frame as vf_sys;
use webrtc_sys::video_track as vt_sys;

#[derive(Clone)]
pub struct NativeVideoSource {
    sys_handle: SharedPtr<vt_sys::ffi::VideoTrackSource>,
}

impl Default for NativeVideoSource {
    fn default() -> Self {
        Self {
            sys_handle: vt_sys::ffi::new_video_track_source(),
        }
    }
}

impl NativeVideoSource {
    pub fn sys_handle(&self) -> SharedPtr<vt_sys::ffi::VideoTrackSource> {
        self.sys_handle.clone()
    }

    pub fn capture_frame<T: AsRef<dyn VideoFrameBuffer>>(&self, frame: &VideoFrame<T>) {
        let mut builder = vf_sys::ffi::new_video_frame_builder();
        builder.pin_mut().set_rotation(frame.rotation.into());
        builder
            .pin_mut()
            .set_video_frame_buffer(frame.buffer.as_ref().sys_handle());
        self.sys_handle
            .on_captured_frame(&builder.pin_mut().build());
    }
}
