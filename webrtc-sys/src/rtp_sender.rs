use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    extern "C++" {
        include!("livekit/webrtc.h");
        include!("livekit/rtp_parameters.h");
        include!("livekit/media_stream.h");

        type MediaType = crate::webrtc::ffi::MediaType;
        type RtpEncodingParameters = crate::rtp_parameters::ffi::RtpEncodingParameters;
        type RtpParameters = crate::rtp_parameters::ffi::RtpParameters;
        type MediaStreamTrack = crate::media_stream::ffi::MediaStreamTrack;
    }

    unsafe extern "C++" {
        include!("livekit/rtp_sender.h");

        type RtpSender;

        fn set_track(self: &RtpSender, track: SharedPtr<MediaStreamTrack>) -> bool;
        fn track(self: &RtpSender) -> SharedPtr<MediaStreamTrack>;
        fn ssrc(self: &RtpSender) -> u32;
        fn media_type(self: &RtpSender) -> MediaType;
        fn id(self: &RtpSender) -> String;
        fn stream_ids(self: &RtpSender) -> Vec<String>;
        fn set_streams(self: &RtpSender, stream_ids: &Vec<String>);
        fn init_send_encodings(self: &RtpSender) -> Vec<RtpEncodingParameters>;
        fn get_parameters(self: &RtpSender) -> RtpParameters;
        fn set_parameters(self: &RtpSender, parameters: RtpParameters) -> Result<()>;

        fn _shared_rtp_sender() -> SharedPtr<RtpSender>;
    }
}

impl_thread_safety!(ffi::RtpSender, Send + Sync);
