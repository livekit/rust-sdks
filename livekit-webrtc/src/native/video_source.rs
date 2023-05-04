use crate::video_frame::{VideoFrame, VideoFrameBuffer};
use cxx::SharedPtr;
use webrtc_sys::media_stream as ms_sys;
use webrtc_sys::video_frame as vf_sys;

#[derive(Clone)]
pub struct NativeVideoSource {
    sys_handle: SharedPtr<ms_sys::ffi::AdaptedVideoTrackSource>,
}

impl Default for NativeVideoSource {
    fn default() -> Self {
        Self {
            sys_handle: ms_sys::ffi::new_adapted_video_track_source(),
        }
    }
}

impl NativeVideoSource {
    pub fn sys_handle(&self) -> SharedPtr<ms_sys::ffi::AdaptedVideoTrackSource> {
        self.sys_handle.clone()
    }

    pub fn capture_frame<T: AsRef<dyn VideoFrameBuffer>>(&self, frame: &VideoFrame<T>) {
        let mut builder = vf_sys::ffi::new_video_frame_builder();
        builder.pin_mut().set_rotation(frame.rotation.into());
        builder
            .pin_mut()
            .set_video_frame_buffer(frame.buffer.as_ref().sys_handle());

        let frame = builder.pin_mut().build();
        self.sys_handle.on_captured_frame(&frame);
    }
}
