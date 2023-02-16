use crate::impl_sys_conversion;
use cxx::SharedPtr;
use webrtc_sys::webrtc as sys_rtc;

#[derive(Debug, Clone, Copy)]
pub enum Priority {
    VeryLow,
    Low,
    Medium,
    High,
}

impl_sys_conversion!(
    sys_rtc::ffi::Priority,
    Priority,
    [VeryLow, Low, Medium, High]
);

#[derive(Debug, Clone, Copy)]
pub enum MediaType {
    Audio,
    Video,
    Data,
    Unsupported,
}

impl_sys_conversion!(
    sys_rtc::ffi::MediaType,
    MediaType,
    [Audio, Video, Data, Unsupported]
);

#[derive(Debug, Clone, Copy)]
pub enum RtpTransceiverDirection {
    SendRecv,
    SendOnly,
    RecvOnly,
    Inactive,
    Stopped,
}

impl_sys_conversion!(
    sys_rtc::ffi::RtpTransceiverDirection,
    RtpTransceiverDirection,
    [SendRecv, SendOnly, RecvOnly, Inactive, Stopped]
);

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
