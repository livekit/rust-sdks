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

    pub fn width(&self) -> u16 {
        self.frame.width()
    }

    pub fn height(&self) -> u16 {
        self.frame.height()
    }

    pub fn first_seq_num(&self) -> u16 {
        self.frame.first_seq_num()
    }

    pub fn last_seq_num(&self) -> u16 {
        self.frame.last_seq_num()
    }

    pub fn get_ntp_time_ms(&self) -> i64 {
        self.frame.get_ntp_time_ms()
    }

    pub fn payload_type(&self) -> u8 {
        self.frame.payload_type()
    }

    pub fn frame_id(&self) -> Option<i64> {
        let value = self.frame.frame_id();
        if !value.is_null() {
            let value = *value;
            return Some(value);
        }
        else {
            return None;
        }
    }

    pub fn temporal_index(&self) -> i32 {
        self.frame.temporal_index()
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