#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    #[derive(Debug)]
    pub struct RtpTransceiverInit {
        pub direction: RtpTransceiverDirection,
        pub stream_ids: Vec<String>,
        pub send_encodings: Vec<RtpEncodingParameters>,
    }

    extern "C++" {
        include!("webrtc-sys/src/webrtc.rs.h");
        include!("webrtc-sys/src/rtp_parameters.rs.h");

        type MediaType = crate::webrtc::ffi::MediaType;
        type RtpTransceiverDirection = crate::webrtc::ffi::RtpTransceiverDirection;
        type RtpEncodingParameters = crate::rtp_parameters::ffi::RtpEncodingParameters;
        type RtpCodecCapability = crate::rtp_parameters::ffi::RtpCodecCapability;
        type RtpHeaderExtensionCapability = crate::rtp_parameters::ffi::RtpHeaderExtensionCapability;
    }

    unsafe extern "C++" {
        include!("livekit/rtp_transceiver.h");
        include!("livekit/rtp_sender.h");
        include!("livekit/rtp_receiver.h");

        type RtpTransceiver;
        type RtpSender = crate::rtp_sender::ffi::RtpSender;
        type RtpReceiver = crate::rtp_receiver::ffi::RtpReceiver;

        fn media_type(self: &RtpTransceiver) -> MediaType;
        fn mid(self: &RtpTransceiver) -> Result<String>;
        fn sender(self: &RtpTransceiver) -> SharedPtr<RtpSender>;
        fn stopped(self: &RtpTransceiver) -> bool;
        fn stopping(self: &RtpTransceiver) -> bool;
        fn direction(self: &RtpTransceiver) -> RtpTransceiverDirection;
        fn set_direction(self: &RtpTransceiver, direction: RtpTransceiverDirection) -> Result<()>;
        fn current_direction(self: &RtpTransceiver) -> Result<RtpTransceiverDirection>;
        fn fired_direction(self: &RtpTransceiver) -> Result<RtpTransceiverDirection>;
        fn stop_standard(self: &RtpTransceiver) -> Result<()>;
        fn set_codec_preferences(self: &RtpTransceiver, codecs: Vec<RtpCodecCapability>) -> Result<()>;
        fn codec_preferences(self: &RtpTransceiver) -> Vec<RtpCodecCapability>;
        fn header_extensions_to_offer(self: &RtpTransceiver) -> Vec<RtpHeaderExtensionCapability>;
        fn header_extensions_negotiated(self: &RtpTransceiver) -> Vec<RtpHeaderExtensionCapability>;
        fn set_offered_rtp_header_extensions(self: &RtpTransceiver, headers: Vec<RtpHeaderExtensionCapability>) -> Result<()>;
    }
}
