// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::rtp_transceiver::RtpTransceiverDirection;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Priority {
    VeryLow,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone)]
pub struct RtpHeaderExtensionParameters {
    pub uri: String,
    pub id: i32,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RtpParameters {
    pub codecs: Vec<RtpCodecParameters>,
    pub header_extensions: Vec<RtpHeaderExtensionParameters>,
    pub encodings: Vec<RtpEncodingParameters>,
    pub rtcp: RtcpParameters,
    /// Opaque token used by WebRTC to pair getParameters/setParameters calls.
    /// Must be preserved when round-tripping through set_parameters().
    pub(crate) transaction_id: String,
    pub(crate) mid: String,
    pub(crate) has_degradation_preference: bool,
    pub(crate) degradation_preference: i32,
}

/// Mirrors webrtc_sys RtcpFeedback for round-trip fidelity.
#[derive(Debug, Clone, Default)]
pub(crate) struct CodecFeedback {
    pub feedback_type: i32,
    pub has_message_type: bool,
    pub message_type: i32,
}

#[derive(Debug, Clone, Default)]
pub struct RtpCodecParameters {
    pub payload_type: u8,
    pub mime_type: String, // read-only
    pub clock_rate: Option<u64>,
    pub channels: Option<u16>,
    pub(crate) name: String,
    pub(crate) kind: i32,
    pub(crate) has_max_ptime: bool,
    pub(crate) max_ptime: i32,
    pub(crate) has_ptime: bool,
    pub(crate) ptime: i32,
    pub(crate) rtcp_feedback: Vec<CodecFeedback>,
    pub(crate) parameters: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct RtcpParameters {
    pub cname: String,
    pub reduced_size: bool,
    pub(crate) mux: bool,
    pub(crate) has_ssrc: bool,
    pub(crate) ssrc: u32,
}

#[derive(Debug, Clone)]
pub struct RtpEncodingParameters {
    pub active: bool,
    pub max_bitrate: Option<u64>,
    pub max_framerate: Option<f64>,
    pub priority: Priority,
    pub rid: String,
    pub scale_resolution_down_by: Option<f64>,
    /// Preserved for round-trip fidelity with WebRTC's getParameters/setParameters.
    pub has_ssrc: bool,
    pub ssrc: u32,
}

#[derive(Debug, Clone)]
pub struct RtpCodecCapability {
    pub channels: Option<u16>,
    pub clock_rate: Option<u64>,
    pub mime_type: String,
    pub sdp_fmtp_line: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RtpHeaderExtensionCapability {
    pub uri: String,
    pub direction: RtpTransceiverDirection,
}

#[derive(Debug, Clone)]
pub struct RtpCapabilities {
    pub codecs: Vec<RtpCodecCapability>,
    pub header_extensions: Vec<RtpHeaderExtensionCapability>,
}

impl Default for RtpEncodingParameters {
    fn default() -> Self {
        Self {
            active: true,
            max_bitrate: None,
            max_framerate: None,
            priority: Priority::Low,
            rid: String::default(),
            scale_resolution_down_by: None,
            has_ssrc: false,
            ssrc: 0,
        }
    }
}
