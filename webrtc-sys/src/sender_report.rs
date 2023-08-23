use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    extern "C++" {
    }

    unsafe extern "C++" {
        include!("livekit/sender_report.h");

        type SenderReport;

        // fn width(self: &SenderReport) -> u16;
        // fn height(self: &SenderReport) -> u16;
        // fn timestamp(self: &SenderReport) -> u32;
    }

    impl UniquePtr<SenderReport> {}
}

impl_thread_safety!(ffi::SenderReport, Send + Sync);