use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    extern "C++" {
    }

    unsafe extern "C++" {
        include!("livekit/sender_report.h");

        type SenderReport;
        fn ssrc(self: &SenderReport) -> u32;        
        fn rtp_timestamp(self: &SenderReport) -> u32;
        fn ntp_time_ms(self: &SenderReport) -> i64;
    }

    impl UniquePtr<SenderReport> {}
}

impl_thread_safety!(ffi::SenderReport, Send + Sync);