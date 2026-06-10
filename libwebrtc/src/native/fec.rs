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

use webrtc_sys::webrtc as sys_rtc;

/// Field trials string enabling FlexFEC-03 send support and advertising
/// video/flexfec-03 in sender capabilities and offers.
pub const FLEXFEC_FIELD_TRIALS: &str =
    "WebRTC-FlexFEC-03/Enabled/WebRTC-FlexFEC-03-Advertised/Enabled/";

/// FEC packet mask type, mirrors webrtc::FecMaskType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FecMaskType {
    /// Mask optimized for random (uniform) packet loss.
    Random,
    /// Mask optimized for bursty/consecutive packet loss.
    Bursty,
}

/// Process-global overrides applied to the FEC protection parameters computed
/// by libwebrtc's default FEC controller. Fields left as `None` keep webrtc's
/// adaptive, loss-based behavior.
#[derive(Debug, Clone, Copy, Default)]
pub struct FecOverrideConfig {
    /// Fixed protection rate, 0-255 (255 ~= 100% protection overhead).
    pub fixed_fec_rate: Option<u8>,
    /// Packet mask type used to build FEC masks.
    pub mask_type: Option<FecMaskType>,
    /// Maximum number of media frames protected by a single FEC block.
    pub max_frames: Option<u32>,
}

impl FecOverrideConfig {
    pub fn has_overrides(&self) -> bool {
        self.fixed_fec_rate.is_some() || self.mask_type.is_some() || self.max_frames.is_some()
    }
}

/// Initializes libwebrtc field trials. Must be called at most once and before
/// the first PeerConnection/Room is created: the trials are read when the
/// PeerConnectionFactory singleton is constructed, so later calls have no
/// effect on it. Returns false if field trials were already initialized.
pub fn init_field_trials(trials: &str) -> bool {
    sys_rtc::ffi::init_field_trials(trials.to_owned())
}

/// Registers process-global FEC parameter overrides. Must be called before the
/// first PeerConnection/Room is created. When any field is set, a custom FEC
/// controller (wrapping webrtc's default) is installed in the
/// PeerConnectionFactory; otherwise webrtc's adaptive behavior is kept.
pub fn set_fec_override(config: FecOverrideConfig) {
    let ffi_config = sys_rtc::ffi::FecOverrideConfig {
        has_fec_rate: config.fixed_fec_rate.is_some(),
        fec_rate: config.fixed_fec_rate.unwrap_or(0),
        has_mask_type: config.mask_type.is_some(),
        mask_type: match config.mask_type {
            Some(FecMaskType::Bursty) => sys_rtc::ffi::FecMaskType::Bursty,
            _ => sys_rtc::ffi::FecMaskType::Random,
        },
        has_max_frames: config.max_frames.is_some(),
        max_frames: config.max_frames.unwrap_or(0),
    };
    sys_rtc::ffi::set_fec_override_config(ffi_config);
}
