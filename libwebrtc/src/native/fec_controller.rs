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
//! exposes the SDK's replacement controller, which protects each video send
//! stream at its configured rate. FlexFEC negotiation is enabled automatically
//! when the peer connection factory is created.
//!
//! Field-trial availability and metrics are process wide because the peer
//! connection factory is a singleton; protection rates are configured per
//! send stream.

use webrtc_sys::fec_controller as sys_fec;

/// Aggregated send side FEC rates reported by the RTP layer across all live
/// video send streams.
#[derive(Debug, Clone, Copy, Default)]
pub struct FecSenderMetrics {
    pub sent_video_rate_bps: u32,
    pub sent_fec_rate_bps: u32,
    pub sent_nack_rate_bps: u32,
    pub active_streams: u32,
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

/// Sets additional WebRTC field trials. Returns `false` when the peer
/// connection factory already exists and the trials can no longer take
/// effect. The `LK_WEBRTC_FIELD_TRIALS` environment variable is appended to
/// whatever is configured here.
pub fn set_field_trials(field_trials: &str) -> bool {
    sys_fec::ffi::set_field_trials(field_trials.to_owned())
}
