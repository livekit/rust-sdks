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

use crate::sys;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RtpTransceiverDirection {
    SendRecv,
    SendOnly,
    RecvOnly,
    Inactive,
    Stopped,
}

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
    pub rtcp: RtcpParameters,
}

#[derive(Debug, Clone, Default)]
pub struct RtpCodecParameters {
    pub payload_type: u8,
    pub mime_type: String, // read-only
    pub clock_rate: Option<u64>,
    pub channels: Option<u16>,
}

#[derive(Debug, Clone, Default)]
pub struct RtcpParameters {
    pub cname: String,
    pub reduced_size: bool,
}

#[derive(Debug, Clone)]
pub struct RtpEncodingParameters {
    pub active: bool,
    pub max_bitrate: Option<u64>,
    pub min_bitrate: Option<u64>,
    pub max_framerate: Option<f64>,
    pub rid: String,
    pub scale_resolution_down_by: Option<f64>,
    pub scalability_mode: Option<String>,
}

#[derive(Debug, Clone)]
pub enum RtcpFeedbackMessageType {
    GenericNack,
    Pli,
    Fir,
}

#[derive(Debug, Clone)]
pub enum RtcpFeedbackType {
    Ccm,
    Lntf,
    Nack,
    Remb,
    TransportCC
}

#[derive(Debug, Clone)]
pub struct RtcpFeedback {
    pub feedback_type: RtcpFeedbackType,
    pub has_message_type: bool,
    pub message_type: RtcpFeedbackMessageType,
}

#[derive(Debug, Clone)]
pub struct RtpCodecCapability {
    pub channels: Option<u16>,
    pub clock_rate: Option<u64>,
    pub mime_type: String,
    pub sdp_fmtp_line: Option<String>,
    pub rtcp_feedback: Vec<RtcpFeedback>,
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
            min_bitrate: None,
            max_bitrate: None,
            max_framerate: None,
            rid: String::default(),
            scale_resolution_down_by: None,
            scalability_mode: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RtpTransceiverInit {
    pub direction: RtpTransceiverDirection,
    pub stream_ids: Vec<String>,
    pub send_encodings: Vec<RtpEncodingParameters>,
}

impl From<sys::lkRtpTransceiverDirection> for RtpTransceiverDirection {
    fn from(state: sys::lkRtpTransceiverDirection) -> Self {
        match state {
            sys::lkRtpTransceiverDirection::LK_RTP_TRANSCEIVER_DIRECTION_SENDRECV => Self::SendRecv,

            sys::lkRtpTransceiverDirection::LK_RTP_TRANSCEIVER_DIRECTION_SENDONLY => Self::SendOnly,
            sys::lkRtpTransceiverDirection::LK_RTP_TRANSCEIVER_DIRECTION_RECVONLY => Self::RecvOnly,

            sys::lkRtpTransceiverDirection::LK_RTP_TRANSCEIVER_DIRECTION_INACTIVE => Self::Inactive,
            sys::lkRtpTransceiverDirection::LK_RTP_TRANSCEIVER_DIRECTION_STOPPED => Self::Stopped,
        }
    }
}

impl From<RtpTransceiverDirection> for sys::lkRtpTransceiverDirection {
    fn from(state: RtpTransceiverDirection) -> Self {
        match state {
            RtpTransceiverDirection::SendRecv => Self::LK_RTP_TRANSCEIVER_DIRECTION_SENDRECV,
            RtpTransceiverDirection::SendOnly => Self::LK_RTP_TRANSCEIVER_DIRECTION_SENDONLY,
            RtpTransceiverDirection::RecvOnly => Self::LK_RTP_TRANSCEIVER_DIRECTION_RECVONLY,
            RtpTransceiverDirection::Inactive => Self::LK_RTP_TRANSCEIVER_DIRECTION_INACTIVE,
            RtpTransceiverDirection::Stopped => Self::LK_RTP_TRANSCEIVER_DIRECTION_STOPPED,
        }
    }
}
