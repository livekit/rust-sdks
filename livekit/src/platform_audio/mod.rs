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

mod error;
mod processing;

pub use error::{AudioError, AudioResult};
pub use processing::{AudioProcessingOptions, AudioProcessingType};

// Re-export RtcAudioSource for convenience
pub use libwebrtc::audio_source::RtcAudioSource;

use std::fmt;
use std::sync::{Arc, Weak};

use lazy_static::lazy_static;
use parking_lot::Mutex;

use crate::rtc_engine::lk_runtime::LkRuntime;

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
/// This handle manages the Platform ADM lifecycle via reference counting.
/// When the first PlatformAudio is created, the Platform ADM is acquired.
/// When the last PlatformAudio is dropped, the Platform ADM is released.
struct PlatformAdmHandle {
    runtime: Arc<LkRuntime>,
}

impl Drop for PlatformAdmHandle {
    fn drop(&mut self) {
        log::debug!("PlatformAdmHandle dropped - releasing Platform ADM");
        // Release Platform ADM reference
        // When ref_count reaches 0, the Platform ADM is terminated
        self.runtime.release_platform_adm();
        log::info!(
            "PlatformAdmHandle: released Platform ADM (ref_count now: {})",
            self.runtime.platform_adm_ref_count()
        );
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
            log::debug!(
                "PlatformAudio: reusing existing handle (ref_count: {})",
                handle.runtime.platform_adm_ref_count()
            );
            // The Platform ADM was already acquired when the handle was created.
            // The Arc reference counting ensures the ADM stays active until all
            // PlatformAudio instances are dropped.
            return Ok(Self { handle });
        }

        // Create new handle and acquire Platform ADM
        log::debug!("PlatformAudio: creating new handle");
        let runtime = LkRuntime::instance();

        // Acquire Platform ADM - this creates the platform-specific audio device module
        // on first call and increments the reference count on subsequent calls.
        // When this fails, it means no audio hardware is available.
        if !runtime.acquire_platform_adm() {
            log::error!("PlatformAudio: failed to acquire Platform ADM");
            return Err(AudioError::PlatformInitFailed);
        }
        log::info!(
            "PlatformAudio: acquired Platform ADM (ref_count: {})",
            runtime.platform_adm_ref_count()
        );

        // Enable ADM recording since PlatformAudio needs microphone access
        // Recording is disabled by default to prevent interference with NativeAudioSource
        runtime.set_adm_recording_enabled(true);
        log::info!("PlatformAudio: enabled ADM recording for microphone capture");

        // Enable ADM playout for platform speakers with AEC
        runtime.set_adm_playout_enabled(true);
        log::info!("PlatformAudio: enabled ADM playout for platform speakers");

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

        let audio = Self { handle };

        // Configure audio processing with platform-appropriate defaults:
        // - iOS: prefer_hardware_processing=true (VPIO is excellent)
        // - Android: prefer_hardware_processing=false (hardware AEC unreliable across devices)
        // - Desktop: prefer_hardware_processing=false (hardware not available anyway)
        if let Err(e) = audio.configure_audio_processing(AudioProcessingOptions::default()) {
            log::warn!("PlatformAudio: failed to configure audio processing: {}", e);
        }

        Ok(audio)
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

    /// Returns the GUID of a recording device by index.
    ///
    /// The GUID is a platform-specific unique identifier that is stable across
    /// device hot-plug events. Use this for persistent device selection instead
    /// of indices, which can change when devices are added or removed.
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
    ///     let name = audio.recording_device_name(i);
    ///     let guid = audio.recording_device_guid(i);
    ///     println!("Mic {}: {} (GUID: {})", i, name, guid);
    /// }
    /// ```
    pub fn recording_device_guid(&self, index: u16) -> String {
        self.handle.runtime.recording_device_guid(index)
    }

    /// Returns the GUID of a playout device by index.
    ///
    /// The GUID is a platform-specific unique identifier that is stable across
    /// device hot-plug events. Use this for persistent device selection instead
    /// of indices, which can change when devices are added or removed.
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
    ///     let name = audio.playout_device_name(i);
    ///     let guid = audio.playout_device_guid(i);
    ///     println!("Speaker {}: {} (GUID: {})", i, name, guid);
    /// }
    /// ```
    pub fn playout_device_guid(&self, index: u16) -> String {
        self.handle.runtime.playout_device_guid(index)
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

        if self.handle.runtime.set_recording_device(index) {
            Ok(())
        } else {
            Err(AudioError::OperationFailed("set_recording_device failed".to_string()))
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

        let runtime = &self.handle.runtime;
        if !runtime.set_playout_device(index) {
            return Err(AudioError::OperationFailed("set_playout_device failed".to_string()));
        }

        // Note: We intentionally do NOT call init_playout()/start_playout() here.
        // On iOS, calling these too early causes a race condition crash in
        // AudioDeviceIOS::OnChangedOutputVolume() because the KVO observers
        // fire before the audio device is fully initialized.
        // WebRTC will automatically initialize and start playout when needed
        // (e.g., when remote audio arrives or when a track is subscribed).

        Ok(())
    }

    /// Selects a recording (microphone) device by GUID.
    ///
    /// This is the preferred method for device selection as GUIDs are stable
    /// across device hot-plug events, unlike indices which can change.
    ///
    /// # Arguments
    ///
    /// * `guid` - Platform-specific device identifier from [`recording_device_guid`]
    ///
    /// # Errors
    ///
    /// - [`AudioError::DeviceNotFound`] if no device matches the GUID
    /// - [`AudioError::OperationFailed`] if device selection fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// // Save the GUID of the preferred microphone
    /// let preferred_guid = audio.recording_device_guid(0);
    /// // Later, select it by GUID (works even if indices changed)
    /// audio.set_recording_device_by_guid(&preferred_guid)?;
    /// ```
    ///
    /// [`recording_device_guid`]: Self::recording_device_guid
    pub fn set_recording_device_by_guid(&self, guid: &str) -> AudioResult<()> {
        if self.handle.runtime.set_recording_device_by_guid(guid) {
            Ok(())
        } else {
            Err(AudioError::DeviceNotFound)
        }
    }

    /// Selects a playout (speaker) device by GUID.
    ///
    /// This is the preferred method for device selection as GUIDs are stable
    /// across device hot-plug events, unlike indices which can change.
    ///
    /// # Arguments
    ///
    /// * `guid` - Platform-specific device identifier from [`playout_device_guid`]
    ///
    /// # Errors
    ///
    /// - [`AudioError::DeviceNotFound`] if no device matches the GUID
    /// - [`AudioError::OperationFailed`] if device selection fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// // Save the GUID of the preferred speakers
    /// let preferred_guid = audio.playout_device_guid(0);
    /// // Later, select it by GUID (works even if indices changed)
    /// audio.set_playout_device_by_guid(&preferred_guid)?;
    /// ```
    ///
    /// [`playout_device_guid`]: Self::playout_device_guid
    pub fn set_playout_device_by_guid(&self, guid: &str) -> AudioResult<()> {
        let runtime = &self.handle.runtime;
        if !runtime.set_playout_device_by_guid(guid) {
            return Err(AudioError::DeviceNotFound);
        }

        // Note: We intentionally do NOT call init_playout()/start_playout() here.
        // On iOS, calling these too early causes a race condition crash in
        // AudioDeviceIOS::OnChangedOutputVolume() because the KVO observers
        // fire before the audio device is fully initialized.
        // WebRTC will automatically initialize and start playout when needed.

        Ok(())
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
            if !runtime.stop_recording() {
                return Err(AudioError::OperationFailed("stop_recording failed".to_string()));
            }
        }

        if !runtime.set_recording_device(index) {
            return Err(AudioError::OperationFailed("set_recording_device failed".to_string()));
        }

        if was_initialized {
            if !runtime.init_recording() {
                return Err(AudioError::OperationFailed("init_recording failed".to_string()));
            }

            if !runtime.start_recording() {
                return Err(AudioError::OperationFailed("start_recording failed".to_string()));
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
            if !runtime.stop_playout() {
                return Err(AudioError::OperationFailed("stop_playout failed".to_string()));
            }
        }

        if !runtime.set_playout_device(index) {
            return Err(AudioError::OperationFailed("set_playout_device failed".to_string()));
        }

        if was_initialized {
            if !runtime.init_playout() {
                return Err(AudioError::OperationFailed("init_playout failed".to_string()));
            }

            if !runtime.start_playout() {
                return Err(AudioError::OperationFailed("start_playout failed".to_string()));
            }
        }

        Ok(())
    }

    // =========================================================================
    // Recording Control
    // =========================================================================

    /// Starts recording from the microphone.
    ///
    /// Recording is automatically started when a track using `RtcAudioSource::Device`
    /// is published. Use this method to resume recording after calling [`stop_recording`].
    ///
    /// This method turns on the system's recording privacy indicator (e.g., the orange
    /// dot on iOS, or the microphone icon on macOS).
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::OperationFailed`] if recording could not be started.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// audio.start_recording()?;  // Resume recording after stop
    /// ```
    ///
    /// [`stop_recording`]: Self::stop_recording
    pub fn start_recording(&self) -> AudioResult<()> {
        let runtime = &self.handle.runtime;

        // Initialize recording if not already initialized
        if !runtime.recording_is_initialized() {
            if !runtime.init_recording() {
                return Err(AudioError::OperationFailed("init_recording failed".to_string()));
            }
        }

        if runtime.start_recording() {
            log::info!("PlatformAudio: started recording");
            Ok(())
        } else {
            Err(AudioError::OperationFailed("start_recording failed".to_string()))
        }
    }

    /// Stops recording from the microphone.
    ///
    /// Use this method to temporarily stop recording without disposing `PlatformAudio`.
    /// This turns off the system's recording privacy indicator (e.g., the orange
    /// dot on iOS, or the microphone icon on macOS).
    ///
    /// Call [`start_recording`] to resume recording.
    ///
    /// # Note
    ///
    /// When recording is stopped, any published audio tracks using `RtcAudioSource::Device`
    /// will send silence. You should typically unpublish the track before stopping recording.
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::OperationFailed`] if recording could not be stopped.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// // ... publish microphone track ...
    ///
    /// // Mute: stop recording to turn off privacy indicator
    /// room.local_participant().unpublish_track(track, false).await?;
    /// audio.stop_recording()?;
    ///
    /// // Unmute: start recording and republish
    /// audio.start_recording()?;
    /// room.local_participant().publish_track(new_track, opts).await?;
    /// ```
    ///
    /// [`start_recording`]: Self::start_recording
    pub fn stop_recording(&self) -> AudioResult<()> {
        let runtime = &self.handle.runtime;
        if runtime.stop_recording() {
            log::info!("PlatformAudio: stopped recording");
            Ok(())
        } else {
            Err(AudioError::OperationFailed("stop_recording failed".to_string()))
        }
    }

    /// Returns whether recording is currently initialized.
    ///
    /// Recording is initialized when [`start_recording`] is called or when
    /// a track using `RtcAudioSource::Device` is published.
    ///
    /// [`start_recording`]: Self::start_recording
    pub fn is_recording_initialized(&self) -> bool {
        self.handle.runtime.recording_is_initialized()
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
            if !runtime.enable_builtin_aec(enable_hw) {
                log::warn!("enable_builtin_aec({}) failed", enable_hw);
            }
        }

        // Enable/disable hardware AGC
        if runtime.builtin_agc_is_available() {
            let enable_hw = use_hardware && options.auto_gain_control;
            if !runtime.enable_builtin_agc(enable_hw) {
                log::warn!("enable_builtin_agc({}) failed", enable_hw);
            }
        }

        // Enable/disable hardware NS
        if runtime.builtin_ns_is_available() {
            let enable_hw = use_hardware && options.noise_suppression;
            if !runtime.enable_builtin_ns(enable_hw) {
                log::warn!("enable_builtin_ns({}) failed", enable_hw);
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
            if !self.handle.runtime.enable_builtin_aec(enable_hw) {
                return Err(AudioError::OperationFailed("enable_builtin_aec failed".to_string()));
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
            if !self.handle.runtime.enable_builtin_agc(enable_hw) {
                return Err(AudioError::OperationFailed("enable_builtin_agc failed".to_string()));
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
            if !self.handle.runtime.enable_builtin_ns(enable_hw) {
                return Err(AudioError::OperationFailed("enable_builtin_ns failed".to_string()));
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
    fn rtc_audio_source_device_variant() {
        let source = RtcAudioSource::Device;
        assert!(matches!(source, RtcAudioSource::Device));
    }
}
