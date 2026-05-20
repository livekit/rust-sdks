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
/// - **iOS**: Hardware processing via VPIO is always used and provides excellent
///   AEC/AGC/NS. The `prefer_hardware_processing` default is `true` on iOS.
///
/// - **Android**: Hardware AEC quality varies significantly across manufacturers
///   and device models. Many devices have broken or poorly-tuned hardware AEC.
///   The default is `false` to use WebRTC's reliable software processing.
///   See: <https://github.com/react-native-webrtc/react-native-webrtc/issues/713>
///
/// - **Desktop** (macOS, Windows, Linux): Hardware processing is not available.
///   WebRTC's software Audio Processing Module (APM) is always used.
///   The `prefer_hardware_processing` setting is ignored.
///
/// # Example
///
/// ```rust,ignore
/// use livekit::AudioProcessingOptions;
///
/// // Use platform-appropriate defaults
/// let opts = AudioProcessingOptions::default();
///
/// // Disable echo cancellation
/// let opts = AudioProcessingOptions {
///     echo_cancellation: false,
///     ..Default::default()
/// };
///
/// // Force software processing on iOS (not recommended)
/// let opts = AudioProcessingOptions {
///     prefer_hardware_processing: false,
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
    /// # Platform Defaults
    ///
    /// - **iOS**: `true` - VPIO hardware processing is excellent and always used.
    ///   Apple's Voice Processing IO unit provides reliable, low-latency AEC/AGC/NS
    ///   that is tightly integrated with the audio hardware.
    ///
    /// - **Android**: `false` - Hardware AEC is unreliable on many devices.
    ///   Quality varies significantly across manufacturers (Samsung, Xiaomi, etc.)
    ///   and even across models from the same manufacturer. WebRTC's software AEC
    ///   provides consistent behavior across all Android devices.
    ///   Reference: Meta found hardware AEC "broken on many combinations of HW + OS"
    ///   when supporting billions of users across thousands of device models.
    ///
    /// - **Desktop**: `false` - Hardware processing is not available.
    ///   This setting is ignored; WebRTC software APM is always used.
    pub prefer_hardware_processing: bool,
}

impl Default for AudioProcessingOptions {
    fn default() -> Self {
        Self {
            echo_cancellation: true,
            noise_suppression: true,
            auto_gain_control: true,
            // iOS: VPIO hardware processing is excellent and tightly integrated.
            // Android: Hardware AEC is unreliable across the fragmented device ecosystem.
            // Desktop: Hardware processing not available, setting is ignored.
            #[cfg(target_os = "ios")]
            prefer_hardware_processing: true,
            #[cfg(not(target_os = "ios"))]
            prefer_hardware_processing: false,
        }
    }
}
