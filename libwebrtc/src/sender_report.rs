use cxx::UniquePtr;
use webrtc_sys::sender_report::ffi::SenderReport as sys_sr;
use std::slice;

pub struct SenderReport {
    pub(crate) sender_report: UniquePtr<sys_sr>,
}

impl SenderReport {
    pub fn new(sender_report: UniquePtr<sys_sr>) -> Self {
        Self {
            sender_report: sender_report
        }
    }

    pub fn ssrc(&self) -> u32 {
        self.sender_report.ssrc()
    }

    pub fn rtp_timestamp(&self) -> u32 {
        self.sender_report.rtp_timestamp()
    }

    pub fn ntp_time_ms(&self) -> i64 {
        self.sender_report.ntp_time_ms()
    }
}