use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    #[derive(Debug)]
    #[repr(i32)]
    pub enum MediaType {
        Audio,
        Video,
        Data,
        Unsupported,
    }

    #[derive(Debug)]
    #[repr(i32)]
    pub enum Priority {
        VeryLow,
        Low,
        Medium,
        High,
    }

    #[derive(Debug)]
    #[repr(i32)]
    pub enum RtpTransceiverDirection {
        SendRecv,
        SendOnly,
        RecvOnly,
        Inactive,
        Stopped,
    }

    unsafe extern "C++" {
        include!("livekit/webrtc.h");
        type RtcRuntime;

        fn create_random_uuid() -> String;
    }
}

impl_thread_safety!(ffi::RtcRuntime, Send + Sync);
