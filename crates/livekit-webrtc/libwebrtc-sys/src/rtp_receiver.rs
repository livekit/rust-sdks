#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/rtp_receiver.h");

        type RtpReceiver;

        fn _unique_rtp_receiver() -> UniquePtr<RtpReceiver>; // Ignore
    }
}
