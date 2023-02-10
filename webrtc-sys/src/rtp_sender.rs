#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    extern "C++" {
        include!("webrtc-sys/src/webrtc.rs.h");
        include!("webrtc-sys/src/rtp_parameters.rs.h");

        type MediaType = crate::webrtc::ffi::MediaType;
        type RtpEncodingParameters = crate::rtp_parameters::ffi::RtpEncodingParameters;
        type RtpParameters = crate::rtp_parameters::ffi::RtpParameters;
    }

    unsafe extern "C++" {
        include!("livekit/media_stream.h");
        include!("livekit/rtp_sender.h");

        type RtpSender;
        type MediaStreamTrack = crate::media_stream::ffi::MediaStreamTrack;

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
