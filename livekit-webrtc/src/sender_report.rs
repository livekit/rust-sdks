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
}