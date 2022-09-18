use cxx::UniquePtr;
use libwebrtc_sys::webrtc as sys_rtc;

pub struct RTCRuntime {
    cxx_handle: UniquePtr<sys_rtc::ffi::RTCRuntime>
}

impl RTCRuntime {
    pub fn new() -> Self {
        Self {
            cxx_handle: sys_rtc::ffi::create_rtc_runtime()
        }
    }
}