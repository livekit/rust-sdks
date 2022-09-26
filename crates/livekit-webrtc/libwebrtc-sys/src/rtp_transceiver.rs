use cxx::UniquePtr;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/rtp_transceiver.h");

        type RtpTransceiver;

        fn _unique_rtp_transceiver() -> UniquePtr<RtpTransceiver>; // Ignore
    }
}
