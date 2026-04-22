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

use crate::imp::audio_source as imp_as;

#[derive(Default, Debug)]
pub struct AudioSourceOptions {
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub auto_gain_control: bool,
}

/// Audio source type for creating audio tracks.
///
/// Choose the appropriate source based on your audio mode:
///
/// | Audio Mode | Source to Use | Description |
/// |------------|---------------|-------------|
/// | Synthetic (default) | `RtcAudioSource::Native(source)` | Manual frame pushing |
/// | Platform | `RtcAudioSource::Device` | Automatic microphone capture |
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum RtcAudioSource {
    /// Native audio source for manual audio frame capture.
    ///
    /// Use this with Synthetic ADM mode (the default). You push audio frames
    /// manually via `NativeAudioSource::capture_frame()`.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use livekit::webrtc::audio_source::native::NativeAudioSource;
    /// use livekit::webrtc::audio_source::{AudioSourceOptions, RtcAudioSource};
    ///
    /// let source = NativeAudioSource::new(
    ///     AudioSourceOptions::default(),
    ///     48000, 2, 100,
    /// );
    /// source.capture_frame(&frame).await?;
    ///
    /// let track = LocalAudioTrack::create_audio_track(
    ///     "audio",
    ///     RtcAudioSource::Native(source),
    /// );
    /// ```
    #[cfg(not(target_arch = "wasm32"))]
    Native(native::NativeAudioSource),

    /// Device audio source - uses Platform ADM for automatic microphone capture.
    ///
    /// Use this with Platform ADM mode. WebRTC automatically captures audio from
    /// the selected recording device (microphone). You do NOT push frames manually.
    ///
    /// # Requirements
    ///
    /// 1. **Enable Platform ADM first:**
    ///    ```rust,no_run
    ///    use livekit::{AudioManager, AudioMode};
    ///    let audio = AudioManager::instance();
    ///    audio.set_mode(AudioMode::Platform)?;
    ///    ```
    ///
    /// 2. **Optionally select a device:**
    ///    ```rust,no_run
    ///    audio.set_recording_device(0)?;
    ///    ```
    ///
    /// 3. **Create track with `Device` source:**
    ///    ```rust,no_run
    ///    use livekit::webrtc::audio_source::RtcAudioSource;
    ///    let track = LocalAudioTrack::create_audio_track(
    ///        "microphone",
    ///        RtcAudioSource::Device,
    ///    );
    ///    ```
    ///
    /// 4. **Reset after disconnect (IMPORTANT for iOS):**
    ///    ```rust,no_run
    ///    room.disconnect().await;
    ///    audio.reset();  // Releases VPIO AudioUnit
    ///    ```
    ///
    /// # Warning
    ///
    /// - Do NOT use `NativeAudioSource` when Platform ADM is active
    /// - Do NOT forget to call `AudioManager::reset()` after disconnecting,
    ///   especially on iOS where VPIO must be released for other audio frameworks
    ///
    /// # Platform Support
    ///
    /// - **iOS**: CoreAudio with VPIO (Voice Processing IO)
    /// - **macOS**: CoreAudio
    /// - **Windows**: WASAPI
    /// - **Linux**: PulseAudio / ALSA
    /// - **Android**: AAudio / OpenSL ES
    #[cfg(not(target_arch = "wasm32"))]
    Device,
}

impl RtcAudioSource {
    /// Set audio processing options.
    /// Note: For `Device` source, options are controlled by the Platform ADM.
    pub fn set_audio_options(&self, options: AudioSourceOptions) {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Native(source) => source.set_audio_options(options),
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Device => {
                // Device source options are managed by the Platform ADM
                // This is a no-op
            }
        }
    }

    /// Get audio processing options.
    /// Note: For `Device` source, returns default options (actual options are managed by ADM).
    pub fn audio_options(&self) -> AudioSourceOptions {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Native(source) => source.audio_options(),
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Device => AudioSourceOptions::default(),
        }
    }

    /// Get the sample rate.
    /// Note: For `Device` source, returns 48000 (default WebRTC sample rate).
    pub fn sample_rate(&self) -> u32 {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Native(source) => source.sample_rate(),
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Device => 48000, // Default WebRTC sample rate
        }
    }

    /// Get the number of channels.
    /// Note: For `Device` source, returns 1 (mono).
    pub fn num_channels(&self) -> u32 {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Native(source) => source.num_channels(),
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Device => 1, // Default to mono
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::fmt::{Debug, Formatter};

    use super::*;
    use crate::{audio_frame::AudioFrame, RtcError};

    #[derive(Clone)]
    pub struct NativeAudioSource {
        pub(crate) handle: imp_as::NativeAudioSource,
    }

    impl Debug for NativeAudioSource {
        fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
            f.debug_struct("NativeAudioSource").finish()
        }
    }

    impl NativeAudioSource {
        pub fn new(
            options: AudioSourceOptions,
            sample_rate: u32,
            num_channels: u32,
            queue_size_ms: u32,
        ) -> NativeAudioSource {
            Self {
                handle: imp_as::NativeAudioSource::new(
                    options,
                    sample_rate,
                    num_channels,
                    queue_size_ms,
                ),
            }
        }

        pub fn clear_buffer(&self) {
            self.handle.clear_buffer()
        }

        pub async fn capture_frame(&self, frame: &AudioFrame<'_>) -> Result<(), RtcError> {
            self.handle.capture_frame(frame).await
        }

        pub fn set_audio_options(&self, options: AudioSourceOptions) {
            self.handle.set_audio_options(options)
        }

        pub fn audio_options(&self) -> AudioSourceOptions {
            self.handle.audio_options()
        }

        pub fn sample_rate(&self) -> u32 {
            self.handle.sample_rate()
        }

        pub fn num_channels(&self) -> u32 {
            self.handle.num_channels()
        }
    }
}
