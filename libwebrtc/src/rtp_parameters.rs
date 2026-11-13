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

/// Controls how the encoder degrades quality when bandwidth is constrained.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum DegradationPreference {
    /// Maintain both framerate and resolution. Frames may be dropped before encoding
    /// if necessary to avoid overusing network and encoder resources.
    MaintainFramerateAndResolution,
    /// Degrade framerate to maintain resolution.
    MaintainFramerate,
    /// Degrade resolution to maintain framerate.
    MaintainResolution,
    /// Balance between framerate and resolution degradation.
    #[default]
    Balanced,
}

impl DegradationPreference {
    /// Deprecated: Use `MaintainFramerateAndResolution` instead.
    #[deprecated(since = "0.8.0", note = "Use MaintainFramerateAndResolution instead")]
    #[allow(non_upper_case_globals)]
    pub const Disabled: Self = Self::MaintainFramerateAndResolution;
}

/// How a [`RtpFecProtection`] policy derives the FEC rate of a video sender.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum FecProtectionMode {
    /// Derive protection from the observed loss and RTT, as when no policy is set.
    #[default]
    Adaptive,
    /// Always request [`RtpFecProtection::protection_factor`], regardless of loss and RTT.
    Fixed,
    /// Request the higher of the adaptive rate and [`RtpFecProtection::protection_factor`].
    Minimum,
    /// Generate no FEC packets, even when FEC was negotiated.
    Disabled,
}

/// Sender-wide FEC (FlexFEC or ULPFEC) protection policy for a video sender.
///
/// Has no effect unless FEC was negotiated. Applied through
/// [`RtpParameters::set_fec_protection`] and takes effect without renegotiation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RtpFecProtection {
    /// How the FEC rate is derived.
    pub mode: FecProtectionMode,
    /// Requested FEC packets relative to media packets, in `[0.0, 1.0]`.
    ///
    /// Used by [`FecProtectionMode::Fixed`] and [`FecProtectionMode::Minimum`].
    pub protection_factor: f64,
    /// Protection factor for key frames. Defaults to `protection_factor`.
    pub key_frame_protection_factor: Option<f64>,
    /// Maximum number of frames covered by one FEC block, in `[1, 6]`.
    ///
    /// Defaults to 1 for [`FecProtectionMode::Fixed`] and the adaptive value otherwise.
    pub max_fec_frames: Option<i32>,
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
    pub(crate) fec_protection: Option<RtpFecProtection>,
}

impl RtpParameters {
    /// Sets the degradation preference for this RTP sender.
    ///
    /// This controls how the encoder trades off between resolution and framerate
    /// when bandwidth is constrained.
    pub fn set_degradation_preference(&mut self, preference: DegradationPreference) {
        self.has_degradation_preference = true;
        self.degradation_preference = match preference {
            DegradationPreference::MaintainFramerateAndResolution => 0,
            DegradationPreference::MaintainFramerate => 1,
            DegradationPreference::MaintainResolution => 2,
            DegradationPreference::Balanced => 3,
        };
    }

    /// Gets the current degradation preference, if set.
    pub fn degradation_preference(&self) -> Option<DegradationPreference> {
        if !self.has_degradation_preference {
            return None;
        }
        Some(match self.degradation_preference {
            0 => DegradationPreference::MaintainFramerateAndResolution,
            1 => DegradationPreference::MaintainFramerate,
            2 => DegradationPreference::MaintainResolution,
            3 => DegradationPreference::Balanced,
            _ => DegradationPreference::Balanced,
        })
    }

    /// Sets the FEC protection policy for this video sender.
    ///
    /// `None` restores adaptive protection. Setting a policy on an audio sender
    /// makes [`set_parameters`](crate::rtp_sender::RtpSender::set_parameters) fail.
    pub fn set_fec_protection(&mut self, protection: Option<RtpFecProtection>) {
        self.fec_protection = protection;
    }

    /// Gets the FEC protection policy, if set.
    pub fn fec_protection(&self) -> Option<&RtpFecProtection> {
        self.fec_protection.as_ref()
    }
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
    /// RTP scalability mode (e.g. "L3T3_KEY"). Required to enable true
    /// SVC for codecs that support it (VP9, AV1).
    pub scalability_mode: Option<String>,
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
            scalability_mode: None,
            has_ssrc: false,
            ssrc: 0,
        }
    }
}
