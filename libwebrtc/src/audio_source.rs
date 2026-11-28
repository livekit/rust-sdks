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
    use crate::imp::audio_source::NativeAudioSource as ImpAudioSource;
    use crate::{audio_frame::AudioFrame, RtcError};

    #[derive(Clone)]
    pub struct NativeAudioSource {
        pub(crate) handle: ImpAudioSource,
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
            Self { handle: ImpAudioSource::new(options, sample_rate, num_channels, queue_size_ms) }
        }

        /// Drops the queued audio and the rest of every capture started before this call.
        pub fn clear_buffer(&self) {
            self.handle.clear_buffer()
        }

        /// Queues `frame`. A frame longer than the queue is fed one queue-sized chunk at a
        /// time; a [`clear_buffer`](Self::clear_buffer) after this call drops the chunks not
        /// queued yet.
        pub fn capture_frame<'a>(
            &self,
            frame: &'a AudioFrame<'a>,
        ) -> impl std::future::Future<Output = Result<(), RtcError>> + Send + 'a {
            self.handle.capture_frame(frame)
        }

        /// [`capture_frame`](Self::capture_frame) with an owned frame, so the future can be
        /// spawned (the FFI's capture requests).
        #[doc(hidden)]
        pub fn capture_owned_frame(
            &self,
            frame: AudioFrame<'static>,
        ) -> impl std::future::Future<Output = Result<(), RtcError>> + Send + 'static {
            self.handle.capture_owned_frame(frame)
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::{
        borrow::Cow,
        time::{Duration, Instant},
    };

    use tokio_stream::StreamExt;

    use super::{native::NativeAudioSource, AudioSourceOptions};
    use crate::{
        audio_frame::AudioFrame,
        audio_stream::native::NativeAudioStream,
        peer_connection_factory::{native::PeerConnectionFactoryExt, PeerConnectionFactory},
    };

    const SAMPLE_RATE: u32 = 24_000;

    /// Audio captured before a clear stops within one 10ms tick of it, even when the capture is
    /// longer than the queue and still feeding it chunk by chunk.
    #[tokio::test(flavor = "multi_thread")]
    async fn clear_buffer_cuts_a_capture_longer_than_the_queue() {
        let factory = PeerConnectionFactory::default();
        let source = NativeAudioSource::new(AudioSourceOptions::default(), SAMPLE_RATE, 1, 50);
        let track = factory.create_audio_track("clear", source.clone());
        let mut stream = NativeAudioStream::new(track, SAMPLE_RATE as i32, 1);

        let samples = SAMPLE_RATE as usize / 2; // 500ms, ten times the queue
        let frame = AudioFrame {
            data: Cow::Owned(vec![1000i16; samples]),
            sample_rate: SAMPLE_RATE,
            num_channels: 1,
            samples_per_channel: samples as u32,
        };
        let capture = tokio::spawn(source.capture_owned_frame(frame));

        tokio::time::sleep(Duration::from_millis(25)).await;
        let cleared = Instant::now();
        source.clear_buffer();
        // The next reply starts at once: the source takes it while the cut capture winds down.
        let next = AudioFrame {
            data: Cow::Owned(vec![0i16; SAMPLE_RATE as usize / 100]),
            sample_rate: SAMPLE_RATE,
            num_channels: 1,
            samples_per_channel: SAMPLE_RATE / 100,
        };
        source.capture_frame(&next).await.expect("a capture right after a clear is taken");
        tokio::time::timeout(Duration::from_millis(100), capture)
            .await
            .expect("a cleared capture resolves")
            .unwrap()
            .unwrap();

        let mut last_audio = None;
        let listen_until = cleared + Duration::from_millis(600);
        while let Ok(Some(frame)) = tokio::time::timeout(
            listen_until.saturating_duration_since(Instant::now()),
            stream.next(),
        )
        .await
        {
            if frame.data.iter().any(|&s| s != 0) {
                last_audio = Some(Instant::now());
            }
        }
        let after_clear =
            last_audio.map_or(Duration::ZERO, |at| at.saturating_duration_since(cleared));
        assert!(
            after_clear < Duration::from_millis(20),
            "audio played {after_clear:?} after the clear"
        );
    }
}
