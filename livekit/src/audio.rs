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

//! Audio device management for the LiveKit SDK.
//!
//! This module provides the [`AudioManager`] for controlling audio device modes
//! and selecting audio devices.
//!
//! # Audio Modes
//!
//! The SDK supports two audio modes:
//!
//! - **Synthetic** (default): Manual audio capture via [`NativeAudioSource`].
//!   Remote participant audio is discarded (not played to speakers).
//!   Use for agents, TTS, file streaming, or testing.
//!
//! - **Platform**: WebRTC's built-in platform audio device management.
//!   WebRTC handles device enumeration, audio capture, and playout automatically.
//!   Use for standard VoIP applications with microphone/speaker support.
//!
//! # Lifecycle and Resource Management
//!
//! **Important for iOS**: Platform mode creates a VPIO (Voice Processing IO)
//! AudioUnit that claims exclusive access to the microphone. Only one VPIO
//! can exist per process. If not properly cleaned up, other audio frameworks
//! will get silence when trying to access the microphone.
//!
//! ## Recommended Teardown Order
//!
//! ```rust,ignore
//! use livekit::{AudioManager, AudioMode, Room};
//!
//! // Setup
//! let audio = AudioManager::instance();
//! audio.set_mode(AudioMode::Platform)?;
//! let (room, events) = Room::connect(&url, &token, options).await?;
//!
//! // ... use room ...
//!
//! // Teardown - IMPORTANT: follow this order
//! // 1. Disconnect from room first
//! room.disconnect().await;
//!
//! // 2. Reset audio to release hardware (VPIO, etc.)
//! audio.reset();
//!
//! // 3. Now other audio frameworks can safely use the microphone
//! ```
//!
//! ## AudioManager Lifetime
//!
//! `AudioManager` holds a reference to the LiveKit runtime. Audio configuration
//! persists as long as the `AudioManager` instance is alive. If you want to
//! release all resources, either:
//! - Call `audio.reset()` to switch back to Synthetic mode, or
//! - Drop the `AudioManager` instance (and ensure no rooms are connected)
//!
//! # Example
//!
//! ```rust,ignore
//! use livekit::{AudioManager, AudioMode};
//!
//! // Get the audio manager instance
//! let audio = AudioManager::instance();
//!
//! // Enable Platform ADM (before connecting to room)
//! audio.set_mode(AudioMode::Platform)?;
//!
//! // Enumerate recording devices
//! for i in 0..audio.recording_devices() as u16 {
//!     println!("Device {}: {}", i, audio.recording_device_name(i));
//! }
//!
//! // Select a recording device
//! audio.set_recording_device(0)?;
//! ```
//!
//! [`NativeAudioSource`]: crate::webrtc::audio_source::native::NativeAudioSource

use std::fmt;

use crate::rtc_engine::lk_runtime::LkRuntime;

// Re-export AdmDelegateType from libwebrtc
pub use libwebrtc::native::AdmDelegateType;

/// Audio device mode selection.
///
/// Determines how audio capture and playout are handled by the SDK.
///
/// # Choosing a Mode
///
/// | Mode | Audio Capture | Audio Playout | AEC | Use Case |
/// |------|---------------|---------------|-----|----------|
/// | Synthetic | Manual (`NativeAudioSource`) | Discarded | No | Agents, TTS, testing |
/// | Platform | Automatic (microphone) | Automatic (speaker) | Yes | VoIP apps |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioMode {
    /// Synthetic ADM - manual audio capture via `NativeAudioSource`.
    ///
    /// This is the **default mode**. Audio is captured by manually pushing
    /// frames to a `NativeAudioSource`.
    ///
    /// # Behavior
    ///
    /// - **Audio capture**: Manual - push frames via `NativeAudioSource::capture_frame()`
    /// - **Audio playout**: Discarded - remote participant audio is NOT played to speakers
    /// - **Echo cancellation (AEC)**: NOT functional (no playout reference)
    /// - **Track creation**: Use `RtcAudioSource::Native(source)`
    ///
    /// # Use Cases
    ///
    /// - Server-side agents that process audio programmatically
    /// - Text-to-speech (TTS) audio streaming
    /// - Audio from files or network streams
    /// - Testing without audio hardware
    /// - Applications that don't need to hear remote participants
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use livekit::prelude::*;
    /// use livekit::webrtc::audio_source::native::NativeAudioSource;
    /// use livekit::webrtc::audio_source::{AudioSourceOptions, RtcAudioSource};
    ///
    /// // Create audio source for manual frame pushing
    /// let source = NativeAudioSource::new(
    ///     AudioSourceOptions::default(),
    ///     48000, 2, 100,
    /// );
    ///
    /// // Push frames manually
    /// source.capture_frame(&audio_frame).await?;
    ///
    /// // Create track with Native source
    /// let track = LocalAudioTrack::create_audio_track(
    ///     "audio",
    ///     RtcAudioSource::Native(source),
    /// );
    /// ```
    #[default]
    Synthetic,

    /// Platform ADM - WebRTC's built-in platform audio device management.
    ///
    /// In this mode, WebRTC handles all audio I/O using the platform's native
    /// audio APIs (CoreAudio on macOS/iOS, WASAPI on Windows, etc.).
    ///
    /// # Behavior
    ///
    /// - **Audio capture**: Automatic - WebRTC captures from selected microphone
    /// - **Audio playout**: Automatic - remote audio plays to selected speaker
    /// - **Echo cancellation (AEC)**: Functional
    /// - **Track creation**: Use `RtcAudioSource::Device`
    ///
    /// # Requirements
    ///
    /// 1. Call `AudioManager::set_mode(AudioMode::Platform)` **before** connecting
    /// 2. Use `RtcAudioSource::Device` when creating audio tracks (NOT `NativeAudioSource`)
    /// 3. Call `AudioManager::reset()` after disconnecting to release hardware
    ///
    /// # Platform-Specific Notes
    ///
    /// - **iOS**: Creates a VPIO (Voice Processing IO) AudioUnit. Only one VPIO
    ///   can exist per process. Other audio frameworks will get silence if VPIO
    ///   is not released via `reset()`.
    /// - **macOS**: Uses CoreAudio for device management.
    /// - **Windows**: Uses WASAPI for device management.
    /// - **Linux**: Uses PulseAudio or ALSA.
    ///
    /// # Use Cases
    ///
    /// - Standard VoIP/video calling applications
    /// - Desktop apps with microphone/speaker device selection
    /// - Applications that need echo cancellation
    /// - Applications where users need to hear remote participants
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use livekit::prelude::*;
    /// use livekit::webrtc::audio_source::RtcAudioSource;
    ///
    /// let audio = AudioManager::instance();
    ///
    /// // 1. Enable Platform mode BEFORE connecting
    /// audio.set_mode(AudioMode::Platform)?;
    ///
    /// // 2. Optionally select devices
    /// audio.set_recording_device(0)?;
    ///
    /// // 3. Connect to room
    /// let (room, _) = Room::connect(&url, &token, options).await?;
    ///
    /// // 4. Create track with Device source (NOT NativeAudioSource!)
    /// let track = LocalAudioTrack::create_audio_track(
    ///     "microphone",
    ///     RtcAudioSource::Device,  // Platform ADM handles capture
    /// );
    ///
    /// // 5. After disconnect, reset to release hardware
    /// room.disconnect().await;
    /// audio.reset();  // IMPORTANT: Release VPIO on iOS
    /// ```
    Platform,
}

impl fmt::Display for AudioMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioMode::Synthetic => write!(f, "Synthetic"),
            AudioMode::Platform => write!(f, "Platform"),
        }
    }
}

/// Errors that can occur during audio operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioError {
    /// Platform ADM could not be initialized.
    ///
    /// This can happen if:
    /// - No audio devices are available
    /// - Audio permissions are not granted
    /// - Platform audio subsystem is unavailable
    PlatformAdmInitFailed,

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
            AudioError::PlatformAdmInitFailed => {
                write!(f, "Failed to initialize platform audio device module")
            }
            AudioError::InvalidDeviceIndex => write!(f, "Invalid device index"),
            AudioError::OperationFailed(msg) => write!(f, "Audio operation failed: {}", msg),
        }
    }
}

impl std::error::Error for AudioError {}

/// Result type for audio operations.
pub type AudioResult<T> = Result<T, AudioError>;

/// Manages audio device modes and device selection.
///
/// `AudioManager` provides a high-level interface for:
/// - Switching between Synthetic and Platform audio modes
/// - Enumerating available audio devices
/// - Selecting recording (microphone) and playout (speaker) devices
///
/// # Process-Global Configuration
///
/// Audio configuration is **process-global** and affects all rooms.
/// The same `AudioManager` instance is shared across the entire process.
///
/// # Usage Pattern
///
/// Configure audio **before** connecting to a room for best results:
///
/// ```rust,ignore
/// use livekit::{AudioManager, AudioMode, Room, RoomOptions};
///
/// // 1. Configure audio BEFORE connecting
/// let audio = AudioManager::instance();
/// audio.set_mode(AudioMode::Platform)?;
/// audio.set_recording_device(0)?;
///
/// // 2. Connect to room
/// let (room, events) = Room::connect(&url, &token, RoomOptions::default()).await?;
///
/// // 3. Create and publish audio track using RtcAudioSource::Device
/// ```
///
/// # Thread Safety
///
/// `AudioManager` is safe to use from multiple threads. All operations
/// are internally synchronized.
#[derive(Clone)]
pub struct AudioManager {
    // Hold a strong reference to LkRuntime to prevent it from being dropped
    // while AudioManager is in use
    runtime: std::sync::Arc<LkRuntime>,
}

impl AudioManager {
    /// Get the `AudioManager` instance.
    ///
    /// This returns a handle to the process-global audio manager.
    /// Multiple calls return handles to the same underlying instance.
    ///
    /// # Note
    ///
    /// The first call to this method will initialize the LiveKit runtime
    /// if it hasn't been initialized already.
    pub fn instance() -> Self {
        Self {
            runtime: LkRuntime::instance(),
        }
    }

    // === Mode Selection ===

    /// Sets the audio device mode.
    ///
    /// Call this **before** connecting to a room for best results.
    /// Mode switching while connected is supported but may briefly interrupt audio.
    ///
    /// # Arguments
    ///
    /// * `mode` - The audio mode to enable
    ///
    /// # Errors
    ///
    /// Returns `AudioError::PlatformAdmInitFailed` if Platform mode cannot be
    /// initialized (e.g., no audio devices available, permissions denied).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use livekit::{AudioManager, AudioMode};
    ///
    /// let audio = AudioManager::instance();
    ///
    /// // Enable Platform ADM for real microphone/speaker support
    /// audio.set_mode(AudioMode::Platform)?;
    /// ```
    pub fn set_mode(&self, mode: AudioMode) -> AudioResult<()> {
        match mode {
            AudioMode::Synthetic => {
                self.runtime.clear_adm_delegate();
                Ok(())
            }
            AudioMode::Platform => {
                if self.runtime.enable_platform_adm() {
                    Ok(())
                } else {
                    Err(AudioError::PlatformAdmInitFailed)
                }
            }
        }
    }

    /// Returns the current audio mode.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use livekit::{AudioManager, AdmDelegateType};
    ///
    /// let audio = AudioManager::instance();
    /// match audio.current_mode() {
    ///     AdmDelegateType::Synthetic => println!("Using synthetic ADM"),
    ///     AdmDelegateType::Platform => println!("Using platform ADM"),
    /// }
    /// ```
    pub fn current_mode(&self) -> AdmDelegateType {
        self.runtime.adm_delegate_type()
    }

    /// Returns `true` if Platform ADM is currently active.
    ///
    /// When this returns `true`, device enumeration and selection methods
    /// will return meaningful results.
    pub fn has_active_adm(&self) -> bool {
        self.runtime.has_adm_delegate()
    }

    // === Device Enumeration ===

    /// Returns the number of available playout (speaker) devices.
    ///
    /// Returns 0 in Synthetic mode or if no devices are available.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use livekit::AudioManager;
    ///
    /// let audio = AudioManager::instance();
    /// println!("Found {} speaker devices", audio.playout_devices());
    /// ```
    pub fn playout_devices(&self) -> i16 {
        self.runtime.playout_devices()
    }

    /// Returns the number of available recording (microphone) devices.
    ///
    /// Returns 0 in Synthetic mode or if no devices are available.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use livekit::AudioManager;
    ///
    /// let audio = AudioManager::instance();
    /// println!("Found {} microphone devices", audio.recording_devices());
    /// ```
    pub fn recording_devices(&self) -> i16 {
        self.runtime.recording_devices()
    }

    /// Returns the name of a playout device by index.
    ///
    /// # Arguments
    ///
    /// * `index` - Device index (0-based)
    ///
    /// # Warning
    ///
    /// Device indices may change when devices are connected/disconnected.
    /// For persistent device selection, match devices by name rather than index.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use livekit::AudioManager;
    ///
    /// let audio = AudioManager::instance();
    /// for i in 0..audio.playout_devices() as u16 {
    ///     println!("Speaker {}: {}", i, audio.playout_device_name(i));
    /// }
    /// ```
    pub fn playout_device_name(&self, index: u16) -> String {
        self.runtime.playout_device_name(index)
    }

    /// Returns the name of a recording device by index.
    ///
    /// # Arguments
    ///
    /// * `index` - Device index (0-based)
    ///
    /// # Warning
    ///
    /// Device indices may change when devices are connected/disconnected.
    /// For persistent device selection, match devices by name rather than index.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use livekit::AudioManager;
    ///
    /// let audio = AudioManager::instance();
    /// for i in 0..audio.recording_devices() as u16 {
    ///     println!("Microphone {}: {}", i, audio.recording_device_name(i));
    /// }
    /// ```
    pub fn recording_device_name(&self, index: u16) -> String {
        self.runtime.recording_device_name(index)
    }

    // === Device Selection ===

    /// Selects a playout (speaker) device by index.
    ///
    /// # Arguments
    ///
    /// * `index` - Device index (0-based, must be < `playout_devices()`)
    ///
    /// # Errors
    ///
    /// Returns `AudioError::InvalidDeviceIndex` if the index is out of range.
    /// Returns `AudioError::OperationFailed` if the device cannot be selected.
    ///
    /// # Warning
    ///
    /// Device indices may change when devices are connected/disconnected.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use livekit::AudioManager;
    ///
    /// let audio = AudioManager::instance();
    /// // Select the first speaker
    /// audio.set_playout_device(0)?;
    /// ```
    pub fn set_playout_device(&self, index: u16) -> AudioResult<()> {
        let count = self.playout_devices();
        if index >= count as u16 {
            return Err(AudioError::InvalidDeviceIndex);
        }

        let result = self.runtime.set_playout_device(index);
        if result == 0 {
            Ok(())
        } else {
            Err(AudioError::OperationFailed(format!(
                "set_playout_device returned {}",
                result
            )))
        }
    }

    /// Selects a recording (microphone) device by index.
    ///
    /// # Arguments
    ///
    /// * `index` - Device index (0-based, must be < `recording_devices()`)
    ///
    /// # Errors
    ///
    /// Returns `AudioError::InvalidDeviceIndex` if the index is out of range.
    /// Returns `AudioError::OperationFailed` if the device cannot be selected.
    ///
    /// # Warning
    ///
    /// Device indices may change when devices are connected/disconnected.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use livekit::AudioManager;
    ///
    /// let audio = AudioManager::instance();
    /// // Select the first microphone
    /// audio.set_recording_device(0)?;
    /// ```
    pub fn set_recording_device(&self, index: u16) -> AudioResult<()> {
        let count = self.recording_devices();
        if index >= count as u16 {
            return Err(AudioError::InvalidDeviceIndex);
        }

        let result = self.runtime.set_recording_device(index);
        if result == 0 {
            Ok(())
        } else {
            Err(AudioError::OperationFailed(format!(
                "set_recording_device returned {}",
                result
            )))
        }
    }

    // === Device Switching (Hot-swap) ===

    /// Switches the recording (microphone) device while audio is active.
    ///
    /// Unlike `set_recording_device()`, this method properly handles the case
    /// where recording is already initialized. It stops recording, changes the
    /// device, and restarts recording.
    ///
    /// # Arguments
    ///
    /// * `index` - Device index (0-based, must be < `recording_devices()`)
    ///
    /// # Errors
    ///
    /// Returns `AudioError::InvalidDeviceIndex` if the index is out of range.
    /// Returns `AudioError::OperationFailed` if any step fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use livekit::AudioManager;
    ///
    /// let audio = AudioManager::instance();
    /// // Switch to a different microphone while in a call
    /// audio.switch_recording_device(1)?;
    /// ```
    pub fn switch_recording_device(&self, index: u16) -> AudioResult<()> {
        let count = self.recording_devices();
        if index >= count as u16 {
            return Err(AudioError::InvalidDeviceIndex);
        }

        // Check if recording is currently initialized
        let was_initialized = self.runtime.recording_is_initialized();

        if was_initialized {
            // Stop recording to clear the initialized state
            let result = self.runtime.stop_recording();
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "stop_recording returned {}",
                    result
                )));
            }
        }

        // Now set the device (should succeed since recording is stopped)
        let result = self.runtime.set_recording_device(index);
        if result != 0 {
            return Err(AudioError::OperationFailed(format!(
                "set_recording_device returned {}",
                result
            )));
        }

        // Re-initialize and start if it was previously initialized
        if was_initialized {
            let result = self.runtime.init_recording();
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "init_recording returned {}",
                    result
                )));
            }

            let result = self.runtime.start_recording();
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "start_recording returned {}",
                    result
                )));
            }
        }

        Ok(())
    }

    /// Switches the playout (speaker) device while audio is active.
    ///
    /// Unlike `set_playout_device()`, this method properly handles the case
    /// where playout is already initialized. It stops playout, changes the
    /// device, and restarts playout.
    ///
    /// # Arguments
    ///
    /// * `index` - Device index (0-based, must be < `playout_devices()`)
    ///
    /// # Errors
    ///
    /// Returns `AudioError::InvalidDeviceIndex` if the index is out of range.
    /// Returns `AudioError::OperationFailed` if any step fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use livekit::AudioManager;
    ///
    /// let audio = AudioManager::instance();
    /// // Switch to a different speaker while in a call
    /// audio.switch_playout_device(1)?;
    /// ```
    pub fn switch_playout_device(&self, index: u16) -> AudioResult<()> {
        let count = self.playout_devices();
        if index >= count as u16 {
            return Err(AudioError::InvalidDeviceIndex);
        }

        // Check if playout is currently initialized
        let was_initialized = self.runtime.playout_is_initialized();

        if was_initialized {
            // Stop playout to clear the initialized state
            let result = self.runtime.stop_playout();
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "stop_playout returned {}",
                    result
                )));
            }
        }

        // Now set the device (should succeed since playout is stopped)
        let result = self.runtime.set_playout_device(index);
        if result != 0 {
            return Err(AudioError::OperationFailed(format!(
                "set_playout_device returned {}",
                result
            )));
        }

        // Re-initialize and start if it was previously initialized
        if was_initialized {
            let result = self.runtime.init_playout();
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "init_playout returned {}",
                    result
                )));
            }

            let result = self.runtime.start_playout();
            if result != 0 {
                return Err(AudioError::OperationFailed(format!(
                    "start_playout returned {}",
                    result
                )));
            }
        }

        Ok(())
    }

    // === Cleanup ===

    /// Resets audio to default state (Synthetic mode), releasing hardware resources.
    ///
    /// **Important**: You MUST call this after disconnecting from a room when using
    /// Platform ADM mode, especially on iOS. Failure to call `reset()` will leave
    /// hardware resources (like VPIO AudioUnit) allocated, preventing other audio
    /// frameworks from accessing the microphone.
    ///
    /// # What This Does
    ///
    /// - Stops audio recording and playout
    /// - Releases platform audio hardware (VPIO on iOS, CoreAudio on macOS, etc.)
    /// - Switches back to Synthetic mode
    /// - Allows other audio frameworks to use the microphone
    ///
    /// # When to Call
    ///
    /// | Scenario | Call `reset()`? |
    /// |----------|-----------------|
    /// | Using Platform ADM and disconnecting | **Yes, required** |
    /// | Using Synthetic mode | No (optional) |
    /// | Reconnecting to another room immediately | No (keep Platform mode) |
    /// | App going to background (iOS) | Yes, recommended |
    /// | Other audio framework needs microphone | **Yes, required** |
    ///
    /// # iOS-Specific Warning
    ///
    /// On iOS, Platform ADM creates a VPIO (Voice Processing IO) AudioUnit that
    /// claims exclusive access to the microphone at the Core Audio level. Only
    /// ONE VPIO can exist per process. If you don't call `reset()`:
    ///
    /// - Other audio frameworks (e.g., speech recognition, other recording libs)
    ///   will receive **silence** when trying to access the microphone
    /// - The VPIO remains allocated until the process terminates
    ///
    /// # Recommended Teardown Order
    ///
    /// ```rust,ignore
    /// use livekit::{AudioManager, AudioMode};
    ///
    /// // 1. Disconnect from room FIRST
    /// room.disconnect().await;
    ///
    /// // 2. Reset audio to release hardware resources
    /// let audio = AudioManager::instance();
    /// audio.reset();
    ///
    /// // 3. Now other audio frameworks can safely use the microphone
    /// // e.g., speech recognition, other recording libraries, etc.
    /// ```
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use livekit::{AudioManager, AudioMode};
    ///
    /// let audio = AudioManager::instance();
    ///
    /// // Setup
    /// audio.set_mode(AudioMode::Platform)?;
    /// let (room, _) = Room::connect(&url, &token, options).await?;
    ///
    /// // ... use room ...
    ///
    /// // Cleanup - IMPORTANT!
    /// room.disconnect().await;
    /// audio.reset();  // Releases VPIO, CoreAudio, WASAPI, etc.
    /// ```
    pub fn reset(&self) {
        self.runtime.clear_adm_delegate();
    }
}

impl fmt::Debug for AudioManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioManager")
            .field("mode", &self.current_mode())
            .field("has_active_adm", &self.has_active_adm())
            .field("recording_devices", &self.recording_devices())
            .field("playout_devices", &self.playout_devices())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_mode_default_is_synthetic() {
        let mode: AudioMode = Default::default();
        assert_eq!(mode, AudioMode::Synthetic);
    }

    #[test]
    fn audio_mode_display() {
        assert_eq!(format!("{}", AudioMode::Synthetic), "Synthetic");
        assert_eq!(format!("{}", AudioMode::Platform), "Platform");
    }

    #[test]
    fn audio_mode_equality() {
        assert_eq!(AudioMode::Synthetic, AudioMode::Synthetic);
        assert_eq!(AudioMode::Platform, AudioMode::Platform);
        assert_ne!(AudioMode::Synthetic, AudioMode::Platform);
    }

    #[test]
    fn audio_mode_clone_and_copy() {
        let mode = AudioMode::Platform;
        let cloned = mode.clone();
        let copied = mode; // Copy

        assert_eq!(mode, cloned);
        assert_eq!(mode, copied);
    }

    #[test]
    fn audio_mode_debug() {
        let mode = AudioMode::Synthetic;
        let debug_str = format!("{:?}", mode);
        assert!(debug_str.contains("Synthetic"));

        let mode = AudioMode::Platform;
        let debug_str = format!("{:?}", mode);
        assert!(debug_str.contains("Platform"));
    }

    #[test]
    fn audio_error_display() {
        let err = AudioError::PlatformAdmInitFailed;
        let msg = format!("{}", err);
        assert!(msg.contains("platform audio device module"));

        let err = AudioError::InvalidDeviceIndex;
        let msg = format!("{}", err);
        assert!(msg.contains("Invalid device index"));

        let err = AudioError::OperationFailed("test message".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("test message"));
    }

    #[test]
    fn audio_error_debug() {
        let err = AudioError::PlatformAdmInitFailed;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("PlatformAdmInitFailed"));

        let err = AudioError::InvalidDeviceIndex;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("InvalidDeviceIndex"));

        let err = AudioError::OperationFailed("test".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("OperationFailed"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn audio_error_equality() {
        assert_eq!(AudioError::PlatformAdmInitFailed, AudioError::PlatformAdmInitFailed);
        assert_eq!(AudioError::InvalidDeviceIndex, AudioError::InvalidDeviceIndex);
        assert_eq!(
            AudioError::OperationFailed("a".to_string()),
            AudioError::OperationFailed("a".to_string())
        );
        assert_ne!(
            AudioError::OperationFailed("a".to_string()),
            AudioError::OperationFailed("b".to_string())
        );
        assert_ne!(AudioError::PlatformAdmInitFailed, AudioError::InvalidDeviceIndex);
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
}
