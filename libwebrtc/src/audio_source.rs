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

use crate::{enum_dispatch, imp::audio_source as imp_as};

/// Default sample rate used by WebRTC audio pipelines (48kHz).
pub const DEFAULT_SAMPLE_RATE: u32 = 48000;

/// Default number of audio channels (mono).
pub const DEFAULT_NUM_CHANNELS: u32 = 1;

#[derive(Default, Debug)]
pub struct AudioSourceOptions {
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub auto_gain_control: bool,
}

/// Audio source type for creating audio tracks.
///
/// Choose the appropriate source based on your use case:
///
/// | Use Case | Source | Description |
/// |----------|--------|-------------|
/// | Manual audio (TTS, files) | `RtcAudioSource::Native(source)` | Push frames manually |
/// | Microphone capture | `RtcAudioSource::Device` | Automatic via Platform ADM |
/// | Both (mic + screen) | Use both types | Multiple tracks supported |
///
/// # Combining Sources
///
/// You can have multiple audio tracks with different source types:
/// - Track A: `RtcAudioSource::Device` for microphone (via `PlatformAudio`)
/// - Track B: `RtcAudioSource::Native` for screen capture or TTS
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
    /// ```rust,ignore
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
    /// WebRTC automatically captures audio from the selected recording device
    /// (microphone). You do NOT push frames manually.
    ///
    /// # Usage
    ///
    /// Use `PlatformAudio` from the `livekit` crate, which manages the Platform ADM
    /// lifecycle and provides `RtcAudioSource::Device` via `rtc_source()`:
    ///
    /// ```rust,ignore
    /// use livekit::prelude::*;
    ///
    /// // Create PlatformAudio (enables Platform ADM)
    /// let audio = PlatformAudio::new()?;
    ///
    /// // Optionally select a specific device
    /// if let Some(device) = audio.recording_devices().next() {
    ///     audio.set_recording_device(&device.id)?;
    /// }
    ///
    /// // Create track using the device source
    /// let track = LocalAudioTrack::create_audio_track("mic", audio.rtc_source());
    /// ```
    ///
    /// # Combining with NativeAudioSource
    ///
    /// You CAN use `NativeAudioSource` alongside Platform ADM to have multiple
    /// audio tracks with different sources (e.g., microphone + screen capture).
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
    /// Note: For `Device` source, returns [`DEFAULT_SAMPLE_RATE`] (48kHz).
    pub fn sample_rate(&self) -> u32 {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Native(source) => source.sample_rate(),
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Device => DEFAULT_SAMPLE_RATE,
        }
    }

    /// Get the number of channels.
    /// Note: For `Device` source, returns [`DEFAULT_NUM_CHANNELS`] (mono).
    pub fn num_channels(&self) -> u32 {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Native(source) => source.num_channels(),
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Device => DEFAULT_NUM_CHANNELS,
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
