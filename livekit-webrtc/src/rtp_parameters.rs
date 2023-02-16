use crate::impl_sys_conversion;
use crate::prelude::*;
use std::collections::HashMap;
use std::vec::Vec;
use webrtc_sys::rtp_parameters as ps_sys;

pub use ps_sys::DEFAULT_BITRATE_PRIORITY;

#[derive(Debug, Clone, Copy)]
pub enum FecMechanism {
    Red,
    RedAndUlpfec,
    FlexFec,
}

impl_sys_conversion!(
    ps_sys::ffi::FecMechanism,
    FecMechanism,
    [Red, RedAndUlpfec, FlexFec]
);

#[derive(Debug, Clone, Copy)]
pub enum RtcpFeedbackType {
    Ccm,
    Lntf,
    Nack,
    Remb,
    TransportCC,
}

impl_sys_conversion!(
    ps_sys::ffi::RtcpFeedbackType,
    RtcpFeedbackType,
    [Ccm, Lntf, Nack, Remb, TransportCC]
);

#[derive(Debug, Clone, Copy)]
pub enum RtcpFeedbackMessageType {
    GenericNack,
    Pli,
    Fir,
}

impl_sys_conversion!(
    ps_sys::ffi::RtcpFeedbackMessageType,
    RtcpFeedbackMessageType,
    [GenericNack, Pli, Fir]
);

#[derive(Debug, Clone, Copy)]
pub enum DegradationPreference {
    Disabled,
    MaintainFramerate,
    MaintainResolution,
    Balanced,
}

impl_sys_conversion!(
    ps_sys::ffi::DegradationPreference,
    DegradationPreference,
    [Disabled, MaintainFramerate, MaintainResolution, Balanced]
);

#[derive(Debug)]
pub enum RtpExtensionFilter {
    DiscardEncryptedExtension,
    PreferEncryptedExtension,
    RequireEncryptedExtension,
}

impl_sys_conversion!(
    ps_sys::ffi::RtpExtensionFilter,
    RtpExtensionFilter,
    [
        DiscardEncryptedExtension,
        PreferEncryptedExtension,
        RequireEncryptedExtension
    ]
);

#[derive(Debug, Clone)]
pub struct RtcpFeedback {
    pub feedback_type: RtcpFeedbackType,
    pub message_type: Option<RtcpFeedbackMessageType>,
}

#[derive(Debug, Clone)]
pub struct RtpCodecCapability {
    pub mime_type: String,
    pub name: String,
    pub kind: MediaType,
    pub clock_rate: Option<i32>,
    pub preferred_payload_type: Option<i32>,
    pub max_ptime: Option<i32>,
    pub ptime: Option<i32>,
    pub num_channels: Option<i32>,
    pub rtcp_feedback: Vec<RtcpFeedback>,
    pub parameters: HashMap<String, String>,
    pub options: HashMap<String, String>,
    pub max_temporal_layer_extensions: i32,
    pub max_spatial_layer_extensions: i32,
    pub svc_multi_stream_support: bool,
}

#[derive(Debug, Clone)]
pub struct RtpHeaderExtensionCapability {
    pub uri: String,
    pub preferred_id: Option<i32>,
    pub preferred_encrypt: bool,
    pub direction: RtpTransceiverDirection,
}

#[derive(Debug, Clone)]
pub struct RtpExtension {
    pub uri: String,
    pub id: i32,
    pub encrypt: bool,
}

#[derive(Debug, Clone)]
pub struct RtpFecParameters {
    pub ssrc: Option<u32>,
    pub mechanism: FecMechanism,
}

#[derive(Debug, Clone)]
pub struct RtpRtxParameters {
    pub ssrc: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct RtpEncodingParameters {
    pub ssrc: Option<u32>,
    pub bitrate_priority: f64,
    pub network_priority: Priority,
    pub max_bitrate_bps: Option<i32>,
    pub min_bitrate_bps: Option<i32>,
    pub max_framerate: Option<f64>,
    pub num_temporal_layers: Option<i32>,
    pub scale_resolution_down_by: Option<f64>,
    pub scalability_mode: Option<String>,
    pub active: bool,
    pub rid: String,
    pub adaptive_ptime: bool,
}

#[derive(Debug, Clone)]
pub struct RtpCodecParameters {
    pub mime_type: String,
    pub name: String,
    pub kind: MediaType,
    pub payload_type: i32,
    pub clock_rate: Option<i32>,
    pub num_channels: Option<i32>,
    pub max_ptime: Option<i32>,
    pub ptime: Option<i32>,
    pub rtcp_feedback: Vec<RtcpFeedback>,
    pub parameters: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct RtpCapabilities {
    pub codecs: Vec<RtpCodecCapability>,
    pub header_extensions: Vec<RtpHeaderExtensionCapability>,
    pub fec: Vec<FecMechanism>,
}

#[derive(Debug, Clone)]
pub struct RtcpParameters {
    pub ssrc: Option<u32>,
    pub cname: String,
    pub reduced_size: bool,
    pub mux: bool,
}

#[derive(Debug, Clone)]
pub struct RtpParameters {
    pub transaction_id: String,
    pub mid: String,
    pub codecs: Vec<RtpCodecParameters>,
    pub header_extensions: Vec<RtpExtension>,
    pub encodings: Vec<RtpEncodingParameters>,
    pub rtcp: RtcpParameters,
    pub degradation_preference: Option<DegradationPreference>,
}

fn into_map(vec: Vec<ps_sys::ffi::StringKeyValue>) -> HashMap<String, String> {
    let mut map = HashMap::with_capacity(vec.len());
    for pair in vec {
        map.insert(pair.key, pair.value);
    }
    map
}

impl From<ps_sys::ffi::RtcpFeedback> for RtcpFeedback {
    fn from(value: ps_sys::ffi::RtcpFeedback) -> Self {
        Self {
            feedback_type: value.feedback_type.into(),
            message_type: value
                .has_message_type
                .then_some(value.message_type)
                .map(Into::into),
        }
    }
}

impl From<ps_sys::ffi::RtpCodecCapability> for RtpCodecCapability {
    fn from(value: ps_sys::ffi::RtpCodecCapability) -> Self {
        Self {
            mime_type: value.mime_type,
            name: value.name,
            kind: value.kind.into(),
            clock_rate: value.has_clock_rate.then_some(value.clock_rate),
            preferred_payload_type: value
                .has_preferred_payload_type
                .then_some(value.preferred_payload_type),
            max_ptime: value.has_max_ptime.then_some(value.max_ptime),
            ptime: value.has_ptime.then_some(value.ptime),
            num_channels: value.has_num_channels.then_some(value.num_channels),
            rtcp_feedback: value.rtcp_feedback.into_iter().map(Into::into).collect(),
            parameters: into_map(value.parameters),
            options: into_map(value.options),
            max_temporal_layer_extensions: value.max_temporal_layer_extensions,
            max_spatial_layer_extensions: value.max_spatial_layer_extensions,
            svc_multi_stream_support: value.svc_multi_stream_support,
        }
    }
}

impl From<ps_sys::ffi::RtpHeaderExtensionCapability> for RtpHeaderExtensionCapability {
    fn from(value: ps_sys::ffi::RtpHeaderExtensionCapability) -> Self {
        Self {
            uri: value.uri,
            preferred_id: value.has_preferred_id.then_some(value.preferred_id),
            preferred_encrypt: value.preferred_encrypt,
            direction: value.direction.into(),
        }
    }
}

impl From<ps_sys::ffi::RtpExtension> for RtpExtension {
    fn from(value: ps_sys::ffi::RtpExtension) -> Self {
        Self {
            uri: value.uri,
            id: value.id,
            encrypt: value.encrypt,
        }
    }
}

impl From<ps_sys::ffi::RtpFecParameters> for RtpFecParameters {
    fn from(value: ps_sys::ffi::RtpFecParameters) -> Self {
        Self {
            ssrc: value.has_ssrc.then_some(value.ssrc),
            mechanism: value.mechanism.into(),
        }
    }
}

impl From<ps_sys::ffi::RtpRtxParameters> for RtpRtxParameters {
    fn from(value: ps_sys::ffi::RtpRtxParameters) -> Self {
        Self {
            ssrc: value.has_ssrc.then_some(value.ssrc),
        }
    }
}

impl From<ps_sys::ffi::RtpEncodingParameters> for RtpEncodingParameters {
    fn from(value: ps_sys::ffi::RtpEncodingParameters) -> Self {
        Self {
            ssrc: value.has_ssrc.then_some(value.ssrc),
            bitrate_priority: value.bitrate_priority,
            network_priority: value.network_priority.into(),
            max_bitrate_bps: value.has_max_bitrate_bps.then_some(value.max_bitrate_bps),
            min_bitrate_bps: value.has_min_bitrate_bps.then_some(value.min_bitrate_bps),
            max_framerate: value.has_max_framerate.then_some(value.max_framerate),
            num_temporal_layers: value
                .has_num_temporal_layers
                .then_some(value.num_temporal_layers),
            scale_resolution_down_by: value
                .has_scale_resolution_down_by
                .then_some(value.scale_resolution_down_by),
            scalability_mode: value.has_scalability_mode.then_some(value.scalability_mode),
            active: value.active,
            rid: value.rid,
            adaptive_ptime: value.adaptive_ptime,
        }
    }
}

impl From<ps_sys::ffi::RtpCodecParameters> for RtpCodecParameters {
    fn from(value: ps_sys::ffi::RtpCodecParameters) -> Self {
        Self {
            mime_type: value.mime_type,
            name: value.name,
            kind: value.kind.into(),
            payload_type: value.payload_type,
            clock_rate: value.has_clock_rate.then_some(value.clock_rate),
            num_channels: value.has_num_channels.then_some(value.num_channels),
            max_ptime: value.has_max_ptime.then_some(value.max_ptime),
            ptime: value.has_ptime.then_some(value.ptime),
            rtcp_feedback: value.rtcp_feedback.into_iter().map(Into::into).collect(),
            parameters: into_map(value.parameters),
        }
    }
}

impl From<ps_sys::ffi::RtpCapabilities> for RtpCapabilities {
    fn from(value: ps_sys::ffi::RtpCapabilities) -> Self {
        Self {
            codecs: value.codecs.into_iter().map(Into::into).collect(),
            header_extensions: value
                .header_extensions
                .into_iter()
                .map(Into::into)
                .collect(),
            fec: value.fec.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ps_sys::ffi::RtcpParameters> for RtcpParameters {
    fn from(value: ps_sys::ffi::RtcpParameters) -> Self {
        Self {
            ssrc: value.has_ssrc.then_some(value.ssrc),
            cname: value.cname,
            reduced_size: value.reduced_size,
            mux: value.mux,
        }
    }
}

impl From<ps_sys::ffi::RtpParameters> for RtpParameters {
    fn from(value: ps_sys::ffi::RtpParameters) -> Self {
        Self {
            transaction_id: value.transaction_id,
            mid: value.mid,
            codecs: value.codecs.into_iter().map(Into::into).collect(),
            header_extensions: value
                .header_extensions
                .into_iter()
                .map(Into::into)
                .collect(),
            encodings: value.encodings.into_iter().map(Into::into).collect(),
            rtcp: value.rtcp.into(),
            degradation_preference: value
                .has_degradation_preference
                .then_some(value.degradation_preference)
                .map(Into::into),
        }
    }
}

// Ignore the value inside unwrap_or for the following implementations

fn into_vec(map: HashMap<String, String>) -> Vec<ps_sys::ffi::StringKeyValue> {
    let mut vec = Vec::with_capacity(map.len());
    for (key, value) in map {
        vec.push(ps_sys::ffi::StringKeyValue { key, value })
    }
    vec
}

impl From<RtcpFeedback> for ps_sys::ffi::RtcpFeedback {
    fn from(value: RtcpFeedback) -> Self {
        Self {
            feedback_type: value.feedback_type.into(),
            has_message_type: value.message_type.is_some(),
            message_type: value
                .message_type
                .unwrap_or(RtcpFeedbackMessageType::GenericNack)
                .into(),
        }
    }
}

impl From<RtpCodecCapability> for ps_sys::ffi::RtpCodecCapability {
    fn from(value: RtpCodecCapability) -> Self {
        Self {
            mime_type: value.mime_type,
            name: value.name,
            kind: value.kind.into(),
            has_clock_rate: value.clock_rate.is_some(),
            clock_rate: value.clock_rate.unwrap_or(0),
            has_preferred_payload_type: value.preferred_payload_type.is_some(),
            preferred_payload_type: value.preferred_payload_type.unwrap_or(0),
            has_max_ptime: value.max_ptime.is_some(),
            max_ptime: value.max_ptime.unwrap_or(0),
            has_ptime: value.ptime.is_some(),
            ptime: value.ptime.unwrap_or(0),
            has_num_channels: value.num_channels.is_some(),
            num_channels: value.num_channels.unwrap_or(0),
            rtcp_feedback: value.rtcp_feedback.into_iter().map(Into::into).collect(),
            parameters: into_vec(value.parameters),
            options: into_vec(value.options),
            max_temporal_layer_extensions: value.max_temporal_layer_extensions,
            max_spatial_layer_extensions: value.max_spatial_layer_extensions,
            svc_multi_stream_support: value.svc_multi_stream_support,
        }
    }
}

impl From<RtpHeaderExtensionCapability> for ps_sys::ffi::RtpHeaderExtensionCapability {
    fn from(value: RtpHeaderExtensionCapability) -> Self {
        Self {
            uri: value.uri,
            has_preferred_id: value.preferred_id.is_some(),
            preferred_id: value.preferred_id.unwrap_or(0),
            preferred_encrypt: value.preferred_encrypt,
            direction: value.direction.into(),
        }
    }
}

impl From<RtpExtension> for ps_sys::ffi::RtpExtension {
    fn from(value: RtpExtension) -> Self {
        Self {
            uri: value.uri,
            id: value.id,
            encrypt: value.encrypt,
        }
    }
}

impl From<RtpFecParameters> for ps_sys::ffi::RtpFecParameters {
    fn from(value: RtpFecParameters) -> Self {
        Self {
            has_ssrc: value.ssrc.is_some(),
            ssrc: value.ssrc.unwrap_or(0),
            mechanism: value.mechanism.into(),
        }
    }
}

impl From<RtpRtxParameters> for ps_sys::ffi::RtpRtxParameters {
    fn from(value: RtpRtxParameters) -> Self {
        Self {
            has_ssrc: value.ssrc.is_some(),
            ssrc: value.ssrc.unwrap_or(0),
        }
    }
}

impl From<RtpEncodingParameters> for ps_sys::ffi::RtpEncodingParameters {
    fn from(value: RtpEncodingParameters) -> Self {
        Self {
            has_ssrc: value.ssrc.is_some(),
            ssrc: value.ssrc.unwrap_or(0),
            bitrate_priority: value.bitrate_priority,
            network_priority: value.network_priority.into(),
            has_max_bitrate_bps: value.max_bitrate_bps.is_some(),
            max_bitrate_bps: value.max_bitrate_bps.unwrap_or(0),
            has_min_bitrate_bps: value.min_bitrate_bps.is_some(),
            min_bitrate_bps: value.min_bitrate_bps.unwrap_or(0),
            has_max_framerate: value.max_framerate.is_some(),
            max_framerate: value.max_framerate.unwrap_or(0.0),
            has_num_temporal_layers: value.num_temporal_layers.is_some(),
            num_temporal_layers: value.num_temporal_layers.unwrap_or(0),
            has_scale_resolution_down_by: value.scale_resolution_down_by.is_some(),
            scale_resolution_down_by: value.scale_resolution_down_by.unwrap_or(0.0),
            has_scalability_mode: value.scalability_mode.is_some(),
            scalability_mode: value.scalability_mode.unwrap_or(String::new()),
            active: value.active,
            rid: value.rid,
            adaptive_ptime: value.adaptive_ptime,
        }
    }
}

impl From<RtpCodecParameters> for ps_sys::ffi::RtpCodecParameters {
    fn from(value: RtpCodecParameters) -> Self {
        Self {
            mime_type: value.mime_type,
            name: value.name,
            kind: value.kind.into(),
            payload_type: value.payload_type,
            has_clock_rate: value.clock_rate.is_some(),
            clock_rate: value.clock_rate.unwrap_or(0),
            has_num_channels: value.num_channels.is_some(),
            num_channels: value.num_channels.unwrap_or(0),
            has_max_ptime: value.max_ptime.is_some(),
            max_ptime: value.max_ptime.unwrap_or(0),
            has_ptime: value.ptime.is_some(),
            ptime: value.ptime.unwrap_or(0),
            rtcp_feedback: value.rtcp_feedback.into_iter().map(Into::into).collect(),
            parameters: into_vec(value.parameters),
        }
    }
}

impl From<RtpCapabilities> for ps_sys::ffi::RtpCapabilities {
    fn from(value: RtpCapabilities) -> Self {
        Self {
            codecs: value.codecs.into_iter().map(Into::into).collect(),
            header_extensions: value
                .header_extensions
                .into_iter()
                .map(Into::into)
                .collect(),
            fec: value.fec.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<RtcpParameters> for ps_sys::ffi::RtcpParameters {
    fn from(value: RtcpParameters) -> Self {
        Self {
            has_ssrc: value.ssrc.is_some(),
            ssrc: value.ssrc.unwrap_or(0),
            cname: value.cname,
            reduced_size: value.reduced_size,
            mux: value.mux,
        }
    }
}

impl From<RtpParameters> for ps_sys::ffi::RtpParameters {
    fn from(value: RtpParameters) -> Self {
        Self {
            transaction_id: value.transaction_id,
            mid: value.mid,
            codecs: value.codecs.into_iter().map(Into::into).collect(),
            header_extensions: value
                .header_extensions
                .into_iter()
                .map(Into::into)
                .collect(),
            encodings: value.encodings.into_iter().map(Into::into).collect(),
            rtcp: value.rtcp.into(),
            has_degradation_preference: value.degradation_preference.is_some(),
            degradation_preference: value
                .degradation_preference
                .unwrap_or(DegradationPreference::Balanced)
                .into(),
        }
    }
}

impl Default for RtpEncodingParameters {
    fn default() -> Self {
        Self {
            ssrc: None,
            bitrate_priority: DEFAULT_BITRATE_PRIORITY,
            network_priority: Priority::Low,
            active: true,
            max_bitrate_bps: None,
            min_bitrate_bps: None,
            max_framerate: None,
            num_temporal_layers: None,
            scale_resolution_down_by: None,
            scalability_mode: None,
            adaptive_ptime: false,
            rid: String::default(),
        }
    }
}
