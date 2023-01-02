#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/webrtc.h");

        type RTCRuntime;

        fn create_rtc_runtime() -> SharedPtr<RTCRuntime>;
    }
}

unsafe impl Send for ffi::RTCRuntime {}

unsafe impl Sync for ffi::RTCRuntime {}
