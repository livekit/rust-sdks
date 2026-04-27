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

//! Platform audio device management for the LiveKit SDK.
//!
//! This module provides [`PlatformAudio`] for accessing platform audio devices
//! (microphones and speakers) via WebRTC's Audio Device Module (ADM).
//!
//! # Overview
//!
//! The SDK supports two ways to handle audio:
//!
//! - **Manual audio** (default): Use [`NativeAudioSource`] to push audio frames manually.
//!   Suitable for agents, TTS, file streaming, or testing.
//!
//! - **Platform audio**: Use [`PlatformAudio`] to capture from microphone and play
//!   to speakers automatically. Suitable for VoIP applications.
//!
//! # Using Platform Audio
//!
//! ```rust,ignore
//! use livekit::prelude::*;
//!
//! // Create PlatformAudio instance (enables platform ADM)
//! let audio = PlatformAudio::new()?;
//!
//! // Enumerate devices
//! for i in 0..audio.recording_devices() as u16 {
//!     println!("Mic {}: {}", i, audio.recording_device_name(i));
//! }
//!
//! // Select a device
//! audio.set_recording_device(0)?;
//!
//! // Create and publish audio track
//! let track = LocalAudioTrack::create_audio_track("mic", audio.rtc_source());
//! room.local_participant().publish_track(LocalTrack::Audio(track), opts).await?;
//!
//! // When audio is dropped, platform ADM is automatically disabled
//! ```
//!
//! # Combining with NativeAudioSource
//!
//! You can use both platform audio and manual audio simultaneously:
//!
//! ```rust,ignore
//! use livekit::prelude::*;
//! use livekit::webrtc::audio_source::native::NativeAudioSource;
//!
//! // Track A: Microphone via platform audio
//! let mic = PlatformAudio::new()?;
//! let mic_track = LocalAudioTrack::create_audio_track("mic", mic.rtc_source());
//!
//! // Track B: Screen capture via manual pushing
//! let screen_source = NativeAudioSource::new(opts, 48000, 2, 100);
//! let screen_track = LocalAudioTrack::create_audio_track(
//!     "screen",
//!     RtcAudioSource::Native(screen_source),
//! );
//!
//! // Publish both
//! room.local_participant().publish_track(LocalTrack::Audio(mic_track), opts).await?;
//! room.local_participant().publish_track(LocalTrack::Audio(screen_track), opts).await?;
//! ```
//!
//! # Reference Counting
//!
//! Multiple [`PlatformAudio`] instances share the same underlying ADM:
//!
//! ```rust,ignore
//! let audio1 = PlatformAudio::new()?;  // Enables ADM
//! let audio2 = PlatformAudio::new()?;  // Reuses same ADM
//! let audio3 = audio1.clone();         // Shares same ADM
//!
//! drop(audio1);
//! drop(audio2);
//! // ADM still active (audio3 holds reference)
//!
//! drop(audio3);
//! // ADM now disabled
//! ```
//!
//! # Platform-Specific Notes
//!
//! - **iOS**: Creates a VPIO (Voice Processing IO) AudioUnit. Only one VPIO
//!   can exist per process. Drop all `PlatformAudio` instances to release it.
//! - **macOS**: Uses CoreAudio.
//! - **Windows**: Uses WASAPI.
//! - **Linux**: Uses PulseAudio or ALSA.
//!
//! [`NativeAudioSource`]: crate::webrtc::audio_source::native::NativeAudioSource

use std::fmt;
use std::sync::{Arc, Weak};

use lazy_static::lazy_static;
use parking_lot::Mutex;

use crate::rtc_engine::lk_runtime::LkRuntime;

// Re-export RtcAudioSource for convenience
pub use libwebrtc::audio_source::RtcAudioSource;

// =============================================================================
// Error Types
// =============================================================================

/// Errors that can occur during audio operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioError {
    /// Platform ADM could not be initialized.
    ///
    /// This can happen if:
    /// - No audio devices are available
    /// - Audio permissions are not granted
    /// - Platform audio subsystem is unavailable
    PlatformInitFailed,

    /// The specified device index is invalid.
    ///
    /// Device indices are 0-based and must be less than the device count.
    InvalidDeviceIndex,

    /// An audio operation failed.
    OperationFailed(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::PlatformInitFailed => {
                write!(f, "Failed to initialize platform audio")
            }
            AudioError::InvalidDeviceIndex => write!(f, "Invalid device index"),
            AudioError::OperationFailed(msg) => write!(f, "Audio operation failed: {}", msg),
        }
    }
}

impl std::error::Error for AudioError {}

/// Result type for audio operations.
pub type AudioResult<T> = Result<T, AudioError>;

// =============================================================================
// Audio Processing Configuration
// =============================================================================

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

// =============================================================================
// PlatformAudio - Reference-counted platform audio device management
// =============================================================================

lazy_static! {
    /// Weak reference to the shared Platform ADM handle.
    /// When all strong references are dropped, the ADM is automatically disabled.
    static ref PLATFORM_ADM_HANDLE: Mutex<Weak<PlatformAdmHandle>> = Mutex::new(Weak::new());
}

/// Internal handle for platform audio.
///
/// This is a marker type that tracks PlatformAudio usage. The Platform ADM
/// is always enabled and does not need to be disabled when dropped.
struct PlatformAdmHandle {
    runtime: Arc<LkRuntime>,
}

impl Drop for PlatformAdmHandle {
    fn drop(&mut self) {
        log::debug!("PlatformAdmHandle dropped");
        // Platform ADM is always enabled, no cleanup needed
    }
}

/// Platform audio device management for microphone capture and speaker playout.
///
/// `PlatformAudio` provides access to the platform's audio devices via WebRTC's
/// Audio Device Module (ADM). Use it to:
///
/// - Enumerate available microphones and speakers
/// - Select which devices to use
/// - Create audio tracks that capture from the microphone
///
/// # Creating a PlatformAudio Instance
///
/// ```rust,ignore
/// use livekit::PlatformAudio;
///
/// let audio = PlatformAudio::new()?;
/// ```
///
/// This enables the platform ADM. If an instance already exists, the new
/// instance shares the same underlying ADM.
///
/// # Device Enumeration
///
/// ```rust,ignore
/// // List microphones
/// for i in 0..audio.recording_devices() as u16 {
///     println!("Mic {}: {}", i, audio.recording_device_name(i));
/// }
///
/// // List speakers
/// for i in 0..audio.playout_devices() as u16 {
///     println!("Speaker {}: {}", i, audio.playout_device_name(i));
/// }
/// ```
///
/// # Device Selection
///
/// ```rust,ignore
/// // Select microphone by index
/// audio.set_recording_device(0)?;
///
/// // Select speaker by index
/// audio.set_playout_device(0)?;
///
/// // Hot-swap devices during active session
/// audio.switch_recording_device(1)?;
/// audio.switch_playout_device(1)?;
/// ```
///
/// # Creating Audio Tracks
///
/// ```rust,ignore
/// use livekit::prelude::*;
///
/// let audio = PlatformAudio::new()?;
/// let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
///
/// room.local_participant()
///     .publish_track(LocalTrack::Audio(track), opts)
///     .await?;
/// ```
///
/// # Lifecycle Management
///
/// `PlatformAudio` uses reference counting. Multiple instances share the same
/// underlying ADM, and the ADM is automatically disabled when all instances
/// are dropped.
///
/// ```rust,ignore
/// let audio1 = PlatformAudio::new()?;  // Enables ADM
/// let audio2 = PlatformAudio::new()?;  // Shares ADM (ref_count = 2)
/// let audio3 = audio1.clone();         // Shares ADM (ref_count = 3)
///
/// drop(audio1);  // ref_count = 2, ADM still active
/// drop(audio2);  // ref_count = 1, ADM still active
/// drop(audio3);  // ref_count = 0, ADM disabled
/// ```
///
/// You can also explicitly release:
///
/// ```rust,ignore
/// audio.release();  // Equivalent to drop(audio)
/// ```
///
/// # Platform-Specific Notes
///
/// - **iOS**: Creates a VPIO AudioUnit (exclusive microphone access).
///   Drop all instances to allow other audio frameworks to use the mic.
/// - **macOS**: Uses CoreAudio for device management.
/// - **Windows**: Uses WASAPI for device management.
/// - **Linux**: Uses PulseAudio or ALSA.
#[derive(Clone)]
pub struct PlatformAudio {
    /// Shared ownership of the Platform ADM handle.
    /// When the last clone is dropped, the ADM is disabled.
    handle: Arc<PlatformAdmHandle>,
}

impl PlatformAudio {
    /// Creates a new `PlatformAudio` instance.
    ///
    /// Platform ADM is always available and initialized at startup.
    /// If another `PlatformAudio` instance exists, this reuses the same handle.
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::PlatformInitFailed`] if no audio devices are available.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use livekit::PlatformAudio;
    ///
    /// let audio = PlatformAudio::new()?;
    /// println!("Found {} microphones", audio.recording_devices());
    /// ```
    pub fn new() -> AudioResult<Self> {
        let mut handle_ref = PLATFORM_ADM_HANDLE.lock();

        // Try to reuse existing handle
        if let Some(handle) = handle_ref.upgrade() {
            log::debug!("PlatformAudio: reusing existing handle");
            return Ok(Self { handle });
        }

        // Create new handle (Platform ADM is always enabled at startup)
        log::debug!("PlatformAudio: creating new handle");
        let runtime = LkRuntime::instance();

        // Enable ADM recording since PlatformAudio needs microphone access
        // Recording is disabled by default to prevent interference with NativeAudioSource
        runtime.set_adm_recording_enabled(true);
        log::info!("PlatformAudio: enabled ADM recording for microphone capture");

        // Verify Platform ADM is working by checking device count
        let recording_count = runtime.recording_devices();
        let playout_count = runtime.playout_devices();
        log::info!(
            "PlatformAudio: {} recording devices, {} playout devices",
            recording_count,
            playout_count
        );

        let handle = Arc::new(PlatformAdmHandle { runtime });
        *handle_ref = Arc::downgrade(&handle);

        Ok(Self { handle })
    }

    // =========================================================================
    // Audio Source
    // =========================================================================

    /// Returns the [`RtcAudioSource`] to use when creating audio tracks.
    ///
    /// This returns `RtcAudioSource::Device`, which tells the track to capture
    /// audio from the platform's selected recording device (microphone).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use livekit::prelude::*;
    ///
    /// let audio = PlatformAudio::new()?;
    /// let track = LocalAudioTrack::create_audio_track("mic", audio.rtc_source());
    /// ```
    pub fn rtc_source(&self) -> RtcAudioSource {
        RtcAudioSource::Device
    }

    // =========================================================================
    // Device Enumeration
    // =========================================================================

    /// Returns the number of available recording (microphone) devices.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// println!("Found {} microphones", audio.recording_devices());
    /// ```
    pub fn recording_devices(&self) -> i16 {
        self.handle.runtime.recording_devices()
    }

    /// Returns the number of available playout (speaker) devices.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// println!("Found {} speakers", audio.playout_devices());
    /// ```
    pub fn playout_devices(&self) -> i16 {
        self.handle.runtime.playout_devices()
    }

    /// Returns the name of a recording device by index.
    ///
    /// # Arguments
    ///
    /// * `index` - Device index (0-based, must be < `recording_devices()`)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// for i in 0..audio.recording_devices() as u16 {
    ///     println!("Mic {}: {}", i, audio.recording_device_name(i));
    /// }
    /// ```
    pub fn recording_device_name(&self, index: u16) -> String {
        self.handle.runtime.recording_device_name(index)
    }

    /// Returns the name of a playout device by index.
    ///
    /// # Arguments
    ///
    /// * `index` - Device index (0-based, must be < `playout_devices()`)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// for i in 0..audio.playout_devices() as u16 {
    ///     println!("Speaker {}: {}", i, audio.playout_device_name(i));
    /// }
    /// ```
    pub fn playout_device_name(&self, index: u16) -> String {
        self.handle.runtime.playout_device_name(index)
    }

    // =========================================================================
    // Device Selection
    // =========================================================================

    /// Selects a recording (microphone) device by index.
    ///
    /// Call this before creating audio tracks to select which microphone to use.
    ///
    /// # Arguments
    ///
    /// * `index` - Device index (0-based, must be < `recording_devices()`)
    ///
    /// # Errors
    ///
    /// - [`AudioError::InvalidDeviceIndex`] if index is out of range
    /// - [`AudioError::OperationFailed`] if device selection fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// audio.set_recording_device(0)?;  // Select first microphone
    /// ```
    pub fn set_recording_device(&self, index: u16) -> AudioResult<()> {
        let count = self.recording_devices();
        if index >= count as u16 {
            return Err(AudioError::InvalidDeviceIndex);
        }

        let result = self.handle.runtime.set_recording_device(index);
        if result == 0 {
            Ok(())
        } else {
            Err(AudioError::OperationFailed(format!(
                "set_recording_device returned {}",
                result
            )))
        }
    }

    /// Selects a playout (speaker) device by index.
    ///
    /// Call this before connecting to select which speaker to use for audio output.
    ///
    /// # Arguments
    ///
    /// * `index` - Device index (0-based, must be < `playout_devices()`)
    ///
    /// # Errors
    ///
    /// - [`AudioError::InvalidDeviceIndex`] if index is out of range
    /// - [`AudioError::OperationFailed`] if device selection fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// audio.set_playout_device(0)?;  // Select first speaker
    /// ```
    pub fn set_playout_device(&self, index: u16) -> AudioResult<()> {
        let count = self.playout_devices();
        if index >= count as u16 {
            return Err(AudioError::InvalidDeviceIndex);
        }

        let result = self.handle.runtime.set_playout_device(index);
        if result == 0 {
            Ok(())
        } else {
            Err(AudioError::OperationFailed(format!(
                "set_playout_device returned {}",
                result
            )))
        }
    }

    /// Switches the recording device while audio is active (hot-swap).
    ///
    /// Unlike [`set_recording_device`], this method handles the stop/change/restart
    /// sequence required when recording is already active.
    ///
    /// # Arguments
    ///
    /// * `index` - Device index (0-based, must be < `recording_devices()`)
    ///
    /// # Errors
    ///
    /// - [`AudioError::InvalidDeviceIndex`] if index is out of range
    /// - [`AudioError::OperationFailed`] if any step fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // During an active call, switch to a different microphone
    /// audio.switch_recording_device(1)?;
    /// ```
    ///
    /// [`set_recording_device`]: Self::set_recording_device
    pub fn switch_recording_device(&self, index: u16) -> AudioResult<()> {
        let count = self.recording_devices();
        if index >= count as u16 {
            return Err(AudioError::InvalidDeviceIndex);
        }

        let runtime = &self.handle.runtime;
        let was_initialized = runtime.recording_is_initialized();

        if was_initialized {
            let result = runtime.stop_recording();
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "stop_recording returned {}",
                    result
                )));
            }
        }

        let result = runtime.set_recording_device(index);
        if result != 0 {
            return Err(AudioError::OperationFailed(format!(
                "set_recording_device returned {}",
                result
            )));
        }

        if was_initialized {
            let result = runtime.init_recording();
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "init_recording returned {}",
                    result
                )));
            }

            let result = runtime.start_recording();
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "start_recording returned {}",
                    result
                )));
            }
        }

        Ok(())
    }

    /// Switches the playout device while audio is active (hot-swap).
    ///
    /// Unlike [`set_playout_device`], this method handles the stop/change/restart
    /// sequence required when playout is already active.
    ///
    /// # Arguments
    ///
    /// * `index` - Device index (0-based, must be < `playout_devices()`)
    ///
    /// # Errors
    ///
    /// - [`AudioError::InvalidDeviceIndex`] if index is out of range
    /// - [`AudioError::OperationFailed`] if any step fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // During an active call, switch to a different speaker
    /// audio.switch_playout_device(1)?;
    /// ```
    ///
    /// [`set_playout_device`]: Self::set_playout_device
    pub fn switch_playout_device(&self, index: u16) -> AudioResult<()> {
        let count = self.playout_devices();
        if index >= count as u16 {
            return Err(AudioError::InvalidDeviceIndex);
        }

        let runtime = &self.handle.runtime;
        let was_initialized = runtime.playout_is_initialized();

        if was_initialized {
            let result = runtime.stop_playout();
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "stop_playout returned {}",
                    result
                )));
            }
        }

        let result = runtime.set_playout_device(index);
        if result != 0 {
            return Err(AudioError::OperationFailed(format!(
                "set_playout_device returned {}",
                result
            )));
        }

        if was_initialized {
            let result = runtime.init_playout();
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "init_playout returned {}",
                    result
                )));
            }

            let result = runtime.start_playout();
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "start_playout returned {}",
                    result
                )));
            }
        }

        Ok(())
    }

    // =========================================================================
    // Lifecycle Management
    // =========================================================================

    /// Returns the number of active references to the platform ADM.
    ///
    /// This includes all `PlatformAudio` instances sharing the same ADM.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio1 = PlatformAudio::new()?;
    /// assert_eq!(audio1.ref_count(), 1);
    ///
    /// let audio2 = audio1.clone();
    /// assert_eq!(audio1.ref_count(), 2);
    /// ```
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.handle)
    }

    /// Explicitly releases this instance's reference to the platform ADM.
    ///
    /// This is equivalent to `drop(self)`. If this is the last reference,
    /// the platform ADM is disabled and hardware resources are released.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// // ... use audio ...
    /// audio.release();  // ADM disabled if this was the last reference
    /// ```
    pub fn release(self) {
        drop(self);
    }

    // =========================================================================
    // Audio Processing (AEC, AGC, NS)
    // =========================================================================

    /// Checks if hardware echo cancellation is available on this device.
    ///
    /// # Platform Behavior
    ///
    /// - **iOS**: Returns `true` (VPIO provides hardware AEC)
    /// - **Android**: Returns `true` on devices with hardware AEC support
    /// - **Desktop**: Returns `false` (hardware AEC not available)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// if audio.is_hardware_aec_available() {
    ///     println!("Hardware AEC is available");
    /// }
    /// ```
    pub fn is_hardware_aec_available(&self) -> bool {
        self.handle.runtime.builtin_aec_is_available()
    }

    /// Checks if hardware automatic gain control is available on this device.
    ///
    /// # Platform Behavior
    ///
    /// - **iOS**: Returns `true` (VPIO provides hardware AGC)
    /// - **Android**: Returns `true` on devices with hardware AGC support
    /// - **Desktop**: Returns `false` (hardware AGC not available)
    pub fn is_hardware_agc_available(&self) -> bool {
        self.handle.runtime.builtin_agc_is_available()
    }

    /// Checks if hardware noise suppression is available on this device.
    ///
    /// # Platform Behavior
    ///
    /// - **iOS**: Returns `true` (VPIO provides hardware NS)
    /// - **Android**: Returns `true` on devices with hardware NS support
    /// - **Desktop**: Returns `false` (hardware NS not available)
    pub fn is_hardware_ns_available(&self) -> bool {
        self.handle.runtime.builtin_ns_is_available()
    }

    /// Gets the type of echo cancellation currently active.
    ///
    /// # Returns
    ///
    /// - [`AudioProcessingType::Hardware`] if hardware AEC is available and enabled
    /// - [`AudioProcessingType::Software`] if using WebRTC's software AEC
    /// - [`AudioProcessingType::None`] if AEC is disabled
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// match audio.active_aec_type() {
    ///     AudioProcessingType::Hardware => println!("Using hardware AEC"),
    ///     AudioProcessingType::Software => println!("Using software AEC"),
    ///     AudioProcessingType::None => println!("AEC disabled"),
    /// }
    /// ```
    pub fn active_aec_type(&self) -> AudioProcessingType {
        if self.is_hardware_aec_available() {
            AudioProcessingType::Hardware
        } else {
            AudioProcessingType::Software
        }
    }

    /// Gets the type of automatic gain control currently active.
    pub fn active_agc_type(&self) -> AudioProcessingType {
        if self.is_hardware_agc_available() {
            AudioProcessingType::Hardware
        } else {
            AudioProcessingType::Software
        }
    }

    /// Gets the type of noise suppression currently active.
    pub fn active_ns_type(&self) -> AudioProcessingType {
        if self.is_hardware_ns_available() {
            AudioProcessingType::Hardware
        } else {
            AudioProcessingType::Software
        }
    }

    /// Configures audio processing with the given options.
    ///
    /// This method configures echo cancellation, noise suppression, and
    /// automatic gain control based on the provided options.
    ///
    /// # Platform Behavior
    ///
    /// - **iOS**: `prefer_hardware_processing` is ignored (always uses VPIO)
    /// - **Android**: When `prefer_hardware_processing` is `false`, hardware
    ///   effects are disabled and WebRTC's software APM is used instead
    /// - **Desktop**: `prefer_hardware_processing` is ignored (hardware not available)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use livekit::{PlatformAudio, AudioProcessingOptions};
    ///
    /// let audio = PlatformAudio::new()?;
    ///
    /// // Use defaults (software processing recommended)
    /// audio.configure_audio_processing(AudioProcessingOptions::default())?;
    ///
    /// // Disable echo cancellation
    /// audio.configure_audio_processing(AudioProcessingOptions {
    ///     echo_cancellation: false,
    ///     ..Default::default()
    /// })?;
    /// ```
    pub fn configure_audio_processing(&self, options: AudioProcessingOptions) -> AudioResult<()> {
        let runtime = &self.handle.runtime;

        // Configure hardware vs software processing preference
        // When prefer_hardware_processing is false, we disable hardware effects
        // to force WebRTC to use its software APM instead
        let use_hardware = options.prefer_hardware_processing;

        // Enable/disable hardware AEC
        // Note: When hardware is disabled, WebRTC automatically falls back to software
        if runtime.builtin_aec_is_available() {
            let enable_hw = use_hardware && options.echo_cancellation;
            let result = runtime.enable_builtin_aec(enable_hw);
            if result != 0 {
                log::warn!("enable_builtin_aec({}) returned {}", enable_hw, result);
            }
        }

        // Enable/disable hardware AGC
        if runtime.builtin_agc_is_available() {
            let enable_hw = use_hardware && options.auto_gain_control;
            let result = runtime.enable_builtin_agc(enable_hw);
            if result != 0 {
                log::warn!("enable_builtin_agc({}) returned {}", enable_hw, result);
            }
        }

        // Enable/disable hardware NS
        if runtime.builtin_ns_is_available() {
            let enable_hw = use_hardware && options.noise_suppression;
            let result = runtime.enable_builtin_ns(enable_hw);
            if result != 0 {
                log::warn!("enable_builtin_ns({}) returned {}", enable_hw, result);
            }
        }

        log::info!(
            "Audio processing configured: AEC={}, AGC={}, NS={}, prefer_hw={}",
            options.echo_cancellation,
            options.auto_gain_control,
            options.noise_suppression,
            options.prefer_hardware_processing
        );

        Ok(())
    }

    /// Enables or disables echo cancellation.
    ///
    /// This is a convenience method equivalent to calling `configure_audio_processing`
    /// with only the `echo_cancellation` field changed.
    ///
    /// # Arguments
    ///
    /// * `enable` - `true` to enable AEC, `false` to disable
    /// * `prefer_hardware` - `true` to prefer hardware AEC on supported devices
    pub fn set_echo_cancellation(&self, enable: bool, prefer_hardware: bool) -> AudioResult<()> {
        if self.is_hardware_aec_available() {
            let enable_hw = enable && prefer_hardware;
            let result = self.handle.runtime.enable_builtin_aec(enable_hw);
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "enable_builtin_aec returned {}",
                    result
                )));
            }
        }
        Ok(())
    }

    /// Enables or disables automatic gain control.
    ///
    /// # Arguments
    ///
    /// * `enable` - `true` to enable AGC, `false` to disable
    /// * `prefer_hardware` - `true` to prefer hardware AGC on supported devices
    pub fn set_auto_gain_control(&self, enable: bool, prefer_hardware: bool) -> AudioResult<()> {
        if self.is_hardware_agc_available() {
            let enable_hw = enable && prefer_hardware;
            let result = self.handle.runtime.enable_builtin_agc(enable_hw);
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "enable_builtin_agc returned {}",
                    result
                )));
            }
        }
        Ok(())
    }

    /// Enables or disables noise suppression.
    ///
    /// # Arguments
    ///
    /// * `enable` - `true` to enable NS, `false` to disable
    /// * `prefer_hardware` - `true` to prefer hardware NS on supported devices
    pub fn set_noise_suppression(&self, enable: bool, prefer_hardware: bool) -> AudioResult<()> {
        if self.is_hardware_ns_available() {
            let enable_hw = enable && prefer_hardware;
            let result = self.handle.runtime.enable_builtin_ns(enable_hw);
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "enable_builtin_ns returned {}",
                    result
                )));
            }
        }
        Ok(())
    }
}

impl fmt::Debug for PlatformAudio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlatformAudio")
            .field("ref_count", &self.ref_count())
            .field("recording_devices", &self.recording_devices())
            .field("playout_devices", &self.playout_devices())
            .finish()
    }
}

/// Resets the platform audio handle references.
///
/// This drops all references to the platform audio handle, allowing
/// a fresh `PlatformAudio` instance to be created. The Platform ADM
/// itself remains active.
///
/// # Example
///
/// ```rust,ignore
/// use livekit::{PlatformAudio, reset_platform_audio};
///
/// let audio = PlatformAudio::new()?;
/// // ... use audio ...
///
/// // Reset handle references
/// reset_platform_audio();
/// ```
pub fn reset_platform_audio() {
    let mut handle_ref = PLATFORM_ADM_HANDLE.lock();
    *handle_ref = Weak::new();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_error_display() {
        let err = AudioError::PlatformInitFailed;
        let msg = format!("{}", err);
        assert!(msg.contains("platform audio"));

        let err = AudioError::InvalidDeviceIndex;
        let msg = format!("{}", err);
        assert!(msg.contains("Invalid device index"));

        let err = AudioError::OperationFailed("test message".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("test message"));
    }

    #[test]
    fn audio_error_debug() {
        let err = AudioError::PlatformInitFailed;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("PlatformInitFailed"));

        let err = AudioError::InvalidDeviceIndex;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("InvalidDeviceIndex"));
    }

    #[test]
    fn audio_error_equality() {
        assert_eq!(AudioError::PlatformInitFailed, AudioError::PlatformInitFailed);
        assert_eq!(AudioError::InvalidDeviceIndex, AudioError::InvalidDeviceIndex);
        assert_eq!(
            AudioError::OperationFailed("a".to_string()),
            AudioError::OperationFailed("a".to_string())
        );
        assert_ne!(
            AudioError::OperationFailed("a".to_string()),
            AudioError::OperationFailed("b".to_string())
        );
    }

    #[test]
    fn audio_error_clone() {
        let err = AudioError::OperationFailed("test".to_string());
        let cloned = err.clone();
        assert_eq!(err, cloned);
    }

    #[test]
    fn audio_error_is_std_error() {
        let err: Box<dyn std::error::Error> = Box::new(AudioError::InvalidDeviceIndex);
        assert!(err.to_string().contains("Invalid device index"));
    }

    #[test]
    fn audio_result_ok() {
        let result: AudioResult<i32> = Ok(42);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn audio_result_err() {
        let result: AudioResult<i32> = Err(AudioError::InvalidDeviceIndex);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), AudioError::InvalidDeviceIndex);
    }

    #[test]
    fn rtc_audio_source_device_variant() {
        let source = RtcAudioSource::Device;
        assert!(matches!(source, RtcAudioSource::Device));
    }
}
