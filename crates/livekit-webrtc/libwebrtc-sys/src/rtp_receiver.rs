#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/rtp_receiver.h");
        include!("livekit/media_stream.h");

        type MediaStreamTrack = crate::media_stream::ffi::MediaStreamTrack;
        type RtpReceiver;

        fn track(self: &RtpReceiver) -> UniquePtr<MediaStreamTrack>;

        fn _unique_rtp_receiver() -> UniquePtr<RtpReceiver>; // Ignore
    }
}

unsafe impl Sync for ffi::RtpReceiver {}

unsafe impl Send for ffi::RtpReceiver {}