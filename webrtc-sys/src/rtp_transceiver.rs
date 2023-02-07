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

        type RtpTransceiverDirection = crate::webrtc::ffi::RtpTransceiverDirection;
        type RtpEncodingParameters = crate::rtp_parameters::ffi::RtpEncodingParameters;
    }

    unsafe extern "C++" {
        include!("livekit/rtp_transceiver.h");

        type RtpTransceiver;

        // TODO Shared RtpTransceiver
    }
}
