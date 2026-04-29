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

//! Audio processing configuration types (AEC, AGC, NS).

/// The type of audio processing being used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioProcessingType {
    /// Hardware audio processing (iOS VPIO, Android hardware effects).
    Hardware,
    /// Software audio processing (WebRTC's built-in APM).
    Software,
    /// Audio processing is not available or disabled.
    None,
}

impl Default for AudioProcessingType {
    fn default() -> Self {
        Self::Software
    }
}

/// Configuration options for audio processing (AEC, AGC, NS).
///
/// # Platform Behavior
///
/// - **iOS**: Hardware processing via VPIO is always used. `prefer_hardware_processing`
///   is ignored since iOS provides excellent hardware AEC/AGC/NS.
///
/// - **Android**: When `prefer_hardware_processing` is `true`, hardware effects are
///   used if available. However, hardware AEC is unreliable on many Android devices,
///   so the default is `false` (software processing).
///
/// - **Desktop** (macOS, Windows, Linux): Hardware processing is not available.
///   WebRTC's software Audio Processing Module (APM) is always used.
///
/// # Example
///
/// ```rust,ignore
/// use livekit::AudioProcessingOptions;
///
/// // Use defaults (software processing, all effects enabled)
/// let opts = AudioProcessingOptions::default();
///
/// // Disable echo cancellation
/// let opts = AudioProcessingOptions {
///     echo_cancellation: false,
///     ..Default::default()
/// };
///
/// // Try hardware processing on Android (use with caution)
/// let opts = AudioProcessingOptions {
///     prefer_hardware_processing: true,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioProcessingOptions {
    /// Enable echo cancellation.
    ///
    /// Echo cancellation removes acoustic echo from the microphone signal,
    /// which occurs when the speaker output is picked up by the microphone.
    ///
    /// Default: `true`
    pub echo_cancellation: bool,

    /// Enable noise suppression.
    ///
    /// Noise suppression reduces background noise in the microphone signal.
    ///
    /// Default: `true`
    pub noise_suppression: bool,

    /// Enable automatic gain control.
    ///
    /// AGC automatically adjusts the microphone volume to maintain
    /// consistent audio levels.
    ///
    /// Default: `true`
    pub auto_gain_control: bool,

    /// Prefer hardware audio processing when available.
    ///
    /// - **iOS**: Ignored (always uses VPIO hardware)
    /// - **Android**: When `true`, uses hardware effects if available.
    ///   Default is `false` because hardware AEC is unreliable on many devices.
    /// - **Desktop**: Ignored (hardware not available)
    ///
    /// Default: `false` (use reliable software processing)
    pub prefer_hardware_processing: bool,
}

impl Default for AudioProcessingOptions {
    fn default() -> Self {
        Self {
            echo_cancellation: true,
            noise_suppression: true,
            auto_gain_control: true,
            prefer_hardware_processing: false,
        }
    }
}
