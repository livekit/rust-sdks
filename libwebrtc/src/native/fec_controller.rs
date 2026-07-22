// Copyright 2026 LiveKit, Inc.
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

//! FlexFEC send side support.
//!
//! libwebrtc keeps FlexFEC behind field trials and only generates FEC after
//! packet loss has been observed (`FecControllerDefault`). This module
//! exposes the SDK's replacement controller which protects video send
//! streams at a fixed user defined rate, plus the field trial hook needed to
//! negotiate flexfec-03 in the first place.
//!
//! Everything here is process wide: the peer connection factory is a
//! process singleton and the FEC controller factory is part of it.

use webrtc_sys::fec_controller as sys_fec;

/// Field trials enabling flexfec-03 negotiation (send and receive).
pub const FLEXFEC_FIELD_TRIALS: &str =
    "WebRTC-FlexFEC-03/Enabled/WebRTC-FlexFEC-03-Advertised/Enabled/";

/// FlexFEC protection configuration for published video.
#[derive(Debug, Clone, Copy)]
pub struct FecControllerConfig {
    /// generate FEC at the configured rate irrespective of observed loss
    pub enabled: bool,
    /// protection factor 0..=255, fraction of media bitrate spent on FEC is
    /// roughly `fec_rate / 255`
    pub fec_rate: u8,
    /// media frames per protection block, 1..=48
    pub max_fec_frames: u8,
    /// optimize packet masks for bursty rather than random loss
    pub bursty_mask: bool,
}

/// Aggregated send side FEC rates reported by the RTP layer across all live
/// video send streams.
#[derive(Debug, Clone, Copy, Default)]
pub struct FecSenderMetrics {
    pub sent_video_rate_bps: u32,
    pub sent_fec_rate_bps: u32,
    pub sent_nack_rate_bps: u32,
    pub active_streams: u32,
}

/// Applies FEC protection parameters, effective immediately for current and
/// future video send streams.
pub fn set_fec_controller_config(config: FecControllerConfig) {
    sys_fec::ffi::set_fec_controller_config(sys_fec::ffi::FecControllerConfig {
        enabled: config.enabled,
        fec_rate: config.fec_rate as i32,
        max_fec_frames: config.max_fec_frames as i32,
        bursty_mask: config.bursty_mask,
    });
}

/// Snapshot of the aggregated send side FEC rates.
pub fn fec_sender_metrics() -> FecSenderMetrics {
    let metrics = sys_fec::ffi::fec_sender_metrics();
    FecSenderMetrics {
        sent_video_rate_bps: metrics.sent_video_rate_bps,
        sent_fec_rate_bps: metrics.sent_fec_rate_bps,
        sent_nack_rate_bps: metrics.sent_nack_rate_bps,
        active_streams: metrics.active_streams,
    }
}

/// Sets WebRTC field trials, e.g. [`FLEXFEC_FIELD_TRIALS`]. Returns `false`
/// when the peer connection factory already exists and the trials can no
/// longer take effect. The `LK_WEBRTC_FIELD_TRIALS` environment variable is
/// appended to whatever is configured here.
pub fn set_field_trials(field_trials: &str) -> bool {
    sys_fec::ffi::set_field_trials(field_trials.to_owned())
}

/// Convenience to enable flexfec-03 negotiation, must run before the first
/// room connection of the process.
pub fn enable_flexfec_field_trials() -> bool {
    set_field_trials(FLEXFEC_FIELD_TRIALS)
}
