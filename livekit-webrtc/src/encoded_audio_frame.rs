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

    pub fn sequence_number(&self) -> u16 {
        self.frame.sequence_number()
    }

    pub fn timestamp(&self) -> u32 {
        self.frame.timestamp()
    }

    pub fn absolute_capture_timestamp(&self) -> Option<u64> {
        let value = self.frame.absolute_capture_timestamp();
        if !value.is_null() {
            let value = *value;
            return Some(value);
        }
        else {
            return None;
        }
    }

    pub fn estimated_capture_clock_offset(&self) -> Option<i64> {
        let value = self.frame.estimated_capture_clock_offset();
        if !value.is_null() {
            let value = *value;
            return Some(value);
        }
        else {
            return None;
        }
    }

}