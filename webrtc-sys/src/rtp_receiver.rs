#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    struct MediaStreamPtr {
        pub ptr: SharedPtr<MediaStream>,
    }

    extern "C++" {
        include!("webrtc-sys/src/webrtc.rs.h");
        include!("webrtc-sys/src/rtp_parameters.rs.h");

        type MediaType = crate::webrtc::ffi::MediaType;
        type RtpParameters = crate::rtp_parameters::ffi::RtpParameters;
    }

    unsafe extern "C++" {
        include!("livekit/rtp_receiver.h");
        include!("livekit/media_stream.h");

        type MediaStreamTrack = crate::media_stream::ffi::MediaStreamTrack;
        type MediaStream = crate::media_stream::ffi::MediaStream;
        type RtpReceiver;

        fn track(self: &RtpReceiver) -> SharedPtr<MediaStreamTrack>;
        fn stream_ids(self: &RtpReceiver) -> Vec<String>;
        fn streams(self: &RtpReceiver) -> Vec<MediaStreamPtr>;
        fn media_type(self: &RtpReceiver) -> MediaType;
        fn id(self: &RtpReceiver) -> String;
        fn get_parameters(self: &RtpReceiver) -> RtpParameters;
        fn set_jitter_buffer_minimum_delay(self: &RtpReceiver, is_some: bool, delay_seconds: f64);
    }
}

impl_thread_safety!(ffi::RtpReceiver, Send + Sync);
