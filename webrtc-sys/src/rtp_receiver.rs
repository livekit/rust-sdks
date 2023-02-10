#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/rtp_receiver.h");
        include!("livekit/media_stream.h");

        type MediaStreamTrack = crate::media_stream::ffi::MediaStreamTrack;
        type RtpReceiver;

        fn track(self: &RtpReceiver) -> SharedPtr<MediaStreamTrack>;
    }
}

impl_thread_safety!(ffi::RtpReceiver, Send + Sync);
