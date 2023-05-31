use cxx::UniquePtr;
use webrtc_sys::encoded_video_frame::ffi::EncodedVideoFrame as sys_ef;

pub struct EncodedVideoFrame {
    pub(crate) frame: UniquePtr<sys_ef>,
}

impl EncodedVideoFrame {
    pub fn new(frame: UniquePtr<sys_ef>) -> Self {
        Self {
            frame: frame
        }
    }

    pub fn is_key_frame(&self) -> bool {
        self.frame.is_key_frame()
    }
}