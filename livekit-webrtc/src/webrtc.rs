use cxx::SharedPtr;

use webrtc_sys::webrtc as sys_rtc;

pub use sys_rtc::ffi::MediaType;
pub use sys_rtc::ffi::Priority;
pub use sys_rtc::ffi::RtpTransceiverDirection;

#[derive(Clone)]
pub struct RTCRuntime {
    cxx_handle: SharedPtr<sys_rtc::ffi::RTCRuntime>,
}

impl RTCRuntime {
    pub fn new() -> Self {
        Self {
            cxx_handle: sys_rtc::ffi::create_rtc_runtime(),
        }
    }

    pub(crate) fn release(self) -> SharedPtr<sys_rtc::ffi::RTCRuntime> {
        self.cxx_handle
    }
}
