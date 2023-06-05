use cxx::UniquePtr;
use webrtc_sys::encoded_audio_frame::ffi::EncodedAudioFrame as sys_ef;
use std::slice;

pub struct EncodedAudioFrame {
    pub(crate) frame: UniquePtr<sys_ef>,
}

impl EncodedAudioFrame {
    pub fn new(frame: UniquePtr<sys_ef>) -> Self {
        Self {
            frame: frame
        }
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

    pub fn payload_type(&self) -> u8 {
        self.frame.payload_type()
    }

    pub fn timestamp(&self) -> u32 {
        self.frame.timestamp()
    }
}