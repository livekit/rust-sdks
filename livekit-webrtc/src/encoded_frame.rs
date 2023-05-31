use cxx::UniquePtr;
use webrtc_sys::encoded_video_frame::ffi::EncodedVideoFrame as sys_ef;
use std::slice;

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

    pub fn payload(&self) -> &[u8] {
        let data = self.frame.payload_data();
        let size = self.frame.payload_size();
        let slice = unsafe {
            assert!(!data.is_null());
            slice::from_raw_parts(data, size as usize)
        };
        return slice;
    }

    // pub fn get_data(&self) {
    //     self.frame.get_data()
    // }
}