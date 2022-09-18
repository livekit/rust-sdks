use cxx::UniquePtr;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/webrtc.h");

        type RTCRuntime;

        fn create_rtc_runtime() -> UniquePtr<RTCRuntime>;
    }
}
