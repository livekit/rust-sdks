
// TODO(theomonnom) Don't use RTCError as Opaque. I should use a Struct and serialize/deserialize when needed for Result<>

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/rtc_error.h");

        type RTCError;

        fn _unique_rtc_error() -> UniquePtr<RTCError>;
    }
}
