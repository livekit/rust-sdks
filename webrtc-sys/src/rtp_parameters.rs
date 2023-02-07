use cxx::UniquePtr;

pub const DEFAULT_BITRATE_PRIORITY: f64 = 1.0;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    // Used to replace std::map
    #[derive(Debug)]
    pub struct StringKeyValue {
        pub key: String,
        pub value: String,
    }

    #[repr(i32)]
    #[derive(Debug)]
    pub enum FecMechanism {
        RED,
        REDAndULPFEC,
        FLEXFEC,
    }

    #[repr(i32)]
    #[derive(Debug)]
    pub enum RtcpFeedbackType {
        CCM,
        LNTF,
        NACK,
        REMB,
        TransportCC,
    }

    #[repr(i32)]
    #[derive(Debug)]
    pub enum RtcpFeedbackMessageType {
        GenericNACK,
        PLI,
        FIR,
    }

    #[repr(i32)]
    #[derive(Debug)]
    pub enum DegradationPreference {
        Disabled,
        MaintainFramerate,
        MaintainResolution,
        Balanced,
    }

    #[derive(Debug)]
    pub struct RtcpFeedback {
        pub feedback_type: RtcpFeedbackType,
        pub has_message_type: bool,
        pub message_type: RtcpFeedbackMessageType,
    }

    #[derive(Debug)]
    pub struct RtpCodecCapability {
        pub mime_type: String, // filled with mime_type fnc
        pub name: String,
        pub kind: MediaType,
        pub has_clock_rate: bool,
        pub clock_rate: i32,
        pub has_preferred_payload_type: bool,
        pub preferred_payload_type: i32,
        pub has_max_ptime: bool,
        pub max_ptime: i32,
        pub has_ptime: bool,
        pub ptime: i32,
        pub has_num_channels: bool,
        pub num_channels: i32,
        pub rtcp_feedback: Vec<RtcpFeedback>,
        pub parameters: Vec<StringKeyValue>,
        pub options: Vec<StringKeyValue>,
        pub max_temporal_layer_extensions: i32,
        pub max_spatial_layer_extensions: i32,
        pub svc_multi_stream_support: bool,
    }

    #[derive(Debug)]
    pub struct RtpHeaderExtensionCapability {
        pub uri: String,
        pub has_preferred_id: bool,
        pub preferred_id: i32,
        pub preferred_encrypt: bool,
        pub direction: RtpTransceiverDirection,
    }

    #[repr(i32)]
    #[derive(Debug)]
    pub enum RtpExtensionFilter {
        DiscardEncryptedExtension,
        PreferEncryptedExtension,
        RequireEncryptedExtension,
    }

    #[derive(Debug)]
    pub struct RtpExtension {
        // TODO(theomonnom): export available URI inside api/rtp_parameters.h
        pub uri: String,
        pub id: i32,
        pub encrypt: bool,
    }

    #[derive(Debug)]
    pub struct RtpFecParameters {
        pub has_ssrc: bool,
        pub ssrc: u32,
        pub mechanism: FecMechanism,
    }

    #[derive(Debug)]
    pub struct RtpRtxParameters {
        pub has_ssrc: bool,
        pub ssrc: u32,
    }

    #[derive(Debug)]
    pub struct RtpEncodingParameters {
        pub has_ssrc: bool,
        pub ssrc: u32,
        pub bitrate_priority: f64,
        pub network_priority: Priority, // Todo link type
        pub has_max_bitrate_bps: bool,
        pub max_bitrate_bps: i32,
        pub has_min_bitrate_bps: bool,
        pub min_bitrate_bps: i32,
        pub has_max_framerate: bool,
        pub max_framerate: f64,
        pub has_num_temporal_layers: bool,
        pub num_temporal_layers: i32,
        pub has_scale_resolution_down_by: bool,
        pub scale_resolution_down_by: f64,
        pub has_scalability_mode: bool,
        pub scalability_mode: String,
        pub active: bool,
        pub rid: String,
        pub adaptive_ptime: bool,
    }

    #[derive(Debug)]
    pub struct RtpCodecParameters {
        pub mime_type: String, // filled with mime_type fnc
        pub name: String,
        pub kind: MediaType,
        pub payload_type: i32,
        pub has_clock_rate: bool,
        pub clock_rate: i32,
        pub has_num_channels: bool,
        pub num_channels: i32,
        pub has_max_ptime: bool,
        pub max_ptime: i32,
        pub has_ptime: bool,
        pub ptime: i32,
        pub rtcp_feedback: Vec<RtcpFeedback>,
        pub parameters: Vec<StringKeyValue>,
    }

    #[derive(Debug)]
    pub struct RtpCapabilities {
        pub codecs: Vec<RtpCodecCapability>,
        pub header_extensions: Vec<RtpHeaderExtensionCapability>,
        pub fec: Vec<FecMechanism>,
    }

    #[derive(Debug)]
    pub struct RtcpParameters {
        pub has_ssrc: bool,
        pub ssrc: u32,
        pub cname: String,
        pub reduced_size: bool,
        pub mux: bool,
    }

    #[derive(Debug)]
    pub struct RtpParameters {
        pub transaction_id: String,
        pub mid: String,
        pub codecs: Vec<RtpCodecParameters>,
        pub header_extensions: Vec<RtpExtension>,
        pub encodings: Vec<RtpEncodingParameters>,
        pub rtcp: RtcpParameters,
        pub has_degradation_preference: bool,
        pub degradation_preference: DegradationPreference,
    }

    extern "C++" {
        include!("webrtc-sys/src/webrtc.rs.h");

        type Priority = crate::webrtc::ffi::Priority;
        type MediaType = crate::webrtc::ffi::MediaType;
        type RtpTransceiverDirection = crate::webrtc::ffi::RtpTransceiverDirection;
    }
}
