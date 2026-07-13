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
//! // Enumerate devices using iterator
//! for device in audio.recording_devices() {
//!     println!("[{}] {} (ID: {})", device.index, device.name, device.id);
//! }
//!
//! // Select a device by ID (type-safe)
//! if let Some(device) = audio.recording_devices().next() {
//!     audio.set_recording_device(&device.id)?;
//! }
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
//! # Muting
//!
//! To mute the microphone, mute the published track (e.g. `LocalAudioTrack::mute()`).
//! The WebRTC voice engine reacts to the track mute state:
//!
//! - **iOS/macOS** (Apple AudioEngine ADM): the microphone is muted in hardware
//!   according to the configured [`MuteMode`]. The default
//!   ([`MuteMode::VoiceProcessing`]) keeps the audio engine running for fast
//!   unmute; use [`MuteMode::RestartEngine`] to turn off the system microphone
//!   privacy indicator while muted.
//! - **Other platforms**: recording is stopped while muted and restarted on
//!   unmute (default WebRTC behavior).
//!
//! ```rust,ignore
//! let audio = PlatformAudio::new()?;
//! audio.set_mute_mode(MuteMode::RestartEngine)?;  // optional, Apple only
//!
//! // ... publish a device track ...
//! track.mute();    // mutes the microphone using the configured mode
//! track.unmute();  // restores capture
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
//! - **iOS**: Uses WebRTC's Apple AudioEngine ADM with platform voice processing.
//!   Drop all `PlatformAudio` instances to release active audio I/O.
//!   Supports [`MuteMode`] configuration for track muting.
//! - **macOS**: Uses WebRTC's Apple AudioEngine ADM. Full device enumeration and selection supported.
//!   Supports [`MuteMode`] configuration for track muting.
//! - **Windows**: Uses WASAPI. Full device enumeration and selection supported.
//! - **Linux**: Uses PulseAudio or ALSA. Full device enumeration and selection supported.
//! - **Android**: Uses Java AudioRecord/AudioTrack via WebRTC's `JavaAudioDeviceModule`.
//!   **Important:** Device enumeration and selection are NOT meaningful on Android.
//!   Android only reports a single "default" device with no name or ID. Audio routing
//!   (speaker, earpiece, Bluetooth, wired headset) is handled by the system via
//!   `AudioManager`, not through WebRTC device selection. To switch outputs on Android,
//!   use Android's `AudioManager.setSpeakerphoneOn()` API instead.
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

// =============================================================================
// Device Types - Newtypes for type-safe device identification
// =============================================================================

/// Unique identifier for a recording (microphone) device.
///
/// This is a type-safe wrapper around the platform-specific device GUID.
/// Obtain this from [`RecordingDeviceInfo`] returned by [`PlatformAudio::recording_devices()`].
///
/// # Platform Notes
///
/// - **Desktop (Windows, macOS, Linux):** Contains a unique GUID that persists across
///   device hot-plug events. Use this for reliable device selection.
/// - **Android:** Always empty. Android doesn't provide device GUIDs and only reports
///   a single "default" device. Device selection on Android is not meaningful.
///
/// # Example
///
/// ```rust,ignore
/// let audio = PlatformAudio::new()?;
/// for device in audio.recording_devices() {
///     println!("{}: {}", device.name, device.id);
///     // Save device.id for later use (desktop only)
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordingDeviceId(String);

impl RecordingDeviceId {
    /// Creates a recording device ID from a raw platform GUID without validation.
    #[doc(hidden)]
    pub fn from_unchecked_guid(guid: &str) -> Self {
        Self(guid.to_string())
    }

    /// Returns the underlying GUID string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RecordingDeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a playout (speaker) device.
///
/// This is a type-safe wrapper around the platform-specific device GUID.
/// Obtain this from [`PlayoutDeviceInfo`] returned by [`PlatformAudio::playout_devices()`].
///
/// # Platform Notes
///
/// - **Desktop (Windows, macOS, Linux):** Contains a unique GUID that persists across
///   device hot-plug events. Use this for reliable device selection.
/// - **Android:** Always empty. Android doesn't provide device GUIDs and only reports
///   a single "default" device. Audio routing (speaker, earpiece, Bluetooth) is handled
///   by the system via `AudioManager`, not through WebRTC device selection.
///
/// # Example
///
/// ```rust,ignore
/// let audio = PlatformAudio::new()?;
/// for device in audio.playout_devices() {
///     println!("{}: {}", device.name, device.id);
///     // Save device.id for later use (desktop only)
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlayoutDeviceId(String);

impl PlayoutDeviceId {
    /// Creates a playout device ID from a raw platform GUID without validation.
    #[doc(hidden)]
    pub fn from_unchecked_guid(guid: &str) -> Self {
        Self(guid.to_string())
    }

    /// Returns the underlying GUID string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PlayoutDeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Information about a recording (microphone) device.
///
/// This struct contains the device's unique identifier, human-readable name,
/// and index. Use the `id` field with [`PlatformAudio::set_recording_device()`]
/// for type-safe device selection.
///
/// # Platform Notes
///
/// - **Desktop (Windows, macOS, Linux):** Full device information is available.
///   The `id` is a unique GUID, and `name` is a descriptive string (e.g., "MacBook Pro Microphone").
/// - **Android:** Only a single device is reported with an empty `id` and `name`.
///   Android does not support app-level microphone selection - the system automatically
///   selects the best input source. This struct is not useful for device pickers on Android.
///
/// # Example
///
/// ```rust,ignore
/// let audio = PlatformAudio::new()?;
/// for device in audio.recording_devices() {
///     println!("[{}] {} (ID: {})", device.index, device.name, device.id);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct RecordingDeviceInfo {
    /// The unique identifier for this device (stable across hot-plug events on desktop; empty on Android).
    pub id: RecordingDeviceId,
    /// Human-readable device name (empty on Android).
    pub name: String,
    /// Device index (may change when devices are added/removed).
    pub index: usize,
}

/// Information about a playout (speaker) device.
///
/// This struct contains the device's unique identifier, human-readable name,
/// and index. Use the `id` field with [`PlatformAudio::set_playout_device()`]
/// for type-safe device selection.
///
/// # Platform Notes
///
/// - **Desktop (Windows, macOS, Linux):** Full device information is available.
///   The `id` is a unique GUID, and `name` is a descriptive string (e.g., "MacBook Pro Speakers").
/// - **Android:** Only a single device is reported with an empty `id` and `name`.
///   Audio routing (speaker, earpiece, Bluetooth, wired headset) is handled by the system
///   via `AudioManager`, not through WebRTC. Use `AudioManager.setSpeakerphoneOn()` to
///   switch between speaker and earpiece on Android.
///
/// # Example
///
/// ```rust,ignore
/// let audio = PlatformAudio::new()?;
/// for device in audio.playout_devices() {
///     println!("[{}] {} (ID: {})", device.index, device.name, device.id);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct PlayoutDeviceInfo {
    /// The unique identifier for this device (stable across hot-plug events on desktop; empty on Android).
    pub id: PlayoutDeviceId,
    /// Human-readable device name (empty on Android).
    pub name: String,
    /// Device index (may change when devices are added/removed).
    pub index: usize,
}

// =============================================================================
// Mute Mode - How the microphone is muted when a device track is muted
// =============================================================================

/// Controls how the microphone is muted when a published audio track using
/// `RtcAudioSource::Device` is muted (Apple platforms only).
///
/// When a local device-audio track is muted (e.g. via `LocalAudioTrack::mute()`),
/// the WebRTC voice engine mutes the microphone in hardware using the configured
/// mode. The audio pipeline stays alive, so unmuting is fast and does not require
/// republishing the track.
///
/// Configure with [`PlatformAudio::set_mute_mode`]. On platforms without the
/// Apple AudioEngine ADM, muting a track stops and restarts recording instead
/// (default WebRTC behavior) and this enum has no effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MuteMode {
    /// Mute through Apple voice processing (VPIO). The audio engine keeps
    /// running and the system microphone privacy indicator stays on while
    /// muted. Lowest mute/unmute latency. This is the default.
    #[default]
    VoiceProcessing,
    /// Mute by disconnecting the input node and restarting the audio engine.
    /// The system microphone privacy indicator turns off while muted, at the
    /// cost of slower mute/unmute transitions.
    RestartEngine,
    /// Mute by setting the internal input mixer volume to zero. The engine
    /// keeps running and the privacy indicator stays on.
    InputMixer,
}

impl MuteMode {
    /// Converts to the raw value used by the native layer.
    fn to_raw(self) -> i32 {
        match self {
            MuteMode::VoiceProcessing => 0,
            MuteMode::RestartEngine => 1,
            MuteMode::InputMixer => 2,
        }
    }

    /// Converts from the raw value used by the native layer.
    fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            0 => Some(MuteMode::VoiceProcessing),
            1 => Some(MuteMode::RestartEngine),
            2 => Some(MuteMode::InputMixer),
            _ => None,
        }
    }
}

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
    // Device level processing configuration last applied via
    // configure_audio_processing, used by the active_*_type getters
    processing_options: Mutex<AudioProcessingOptions>,
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
/// for device in audio.recording_devices() {
///     println!("Mic {}: {}", device.index, device.name);
/// }
///
/// // List speakers
/// for device in audio.playout_devices() {
///     println!("Speaker {}: {}", device.index, device.name);
/// }
/// ```
///
/// # Device Selection
///
/// ```rust,ignore
/// if let Some(device) = audio.recording_devices().next() {
///     audio.set_recording_device(&device.id)?;
/// }
///
/// // Hot-swap devices during active session
/// let devices: Vec<_> = audio.recording_devices().collect();
/// if let Some(device) = devices.get(1) {
///     audio.switch_recording_device(&device.id)?;
/// }
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
/// - **iOS**: Uses WebRTC's Apple AudioEngine ADM with platform voice processing.
///   Drop all instances to allow other audio frameworks to use the mic.
/// - **macOS**: Uses WebRTC's Apple AudioEngine ADM for device management.
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

        let handle = Arc::new(PlatformAdmHandle {
            runtime,
            processing_options: Mutex::new(AudioProcessingOptions::default()),
        });
        *handle_ref = Arc::downgrade(&handle);

        let audio = Self { handle };

        // Configure audio processing with platform-appropriate defaults:
        // - iOS/macOS: prefer_hardware_processing=true (Apple voice processing is preferred)
        // - Android: prefer_hardware_processing=false (hardware AEC unreliable across devices)
        // - Windows/Linux: prefer_hardware_processing=false (hardware not available)
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

    /// Returns an iterator over available recording (microphone) devices.
    ///
    /// Each [`RecordingDeviceInfo`] contains the device's unique ID, name, and index.
    /// Use the `id` field with [`set_recording_device()`] for type-safe device selection.
    ///
    /// # Platform Notes
    ///
    /// **Desktop (Windows, macOS, Linux):** Full device enumeration is supported.
    /// You can enumerate USB microphones, built-in mics, audio interfaces, etc.
    /// Each device has a unique ID (GUID) and descriptive name.
    ///
    /// **Android:** Only a single "default" device is reported with an empty name and ID.
    /// Android does not support app-level microphone selection - the system automatically
    /// selects the best input source based on the audio mode and connected accessories.
    /// Device enumeration on Android is not meaningful for user-facing device pickers.
    ///
    /// **iOS:** Similar to desktop - devices can be enumerated, though typically only
    /// the built-in microphone and any connected accessories are available.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// for device in audio.recording_devices() {
    ///     println!("[{}] {} (ID: {})", device.index, device.name, device.id);
    /// }
    ///
    /// // Collect into a Vec for later use
    /// let devices: Vec<_> = audio.recording_devices().collect();
    /// ```
    ///
    /// [`set_recording_device()`]: Self::set_recording_device
    pub fn recording_devices(&self) -> impl Iterator<Item = RecordingDeviceInfo> + '_ {
        let count = self.recording_device_count();
        (0..count).filter_map(move |index| self.recording_device_info(index))
    }

    /// Returns an iterator over available playout (speaker) devices.
    ///
    /// Each [`PlayoutDeviceInfo`] contains the device's unique ID, name, and index.
    /// Use the `id` field with [`set_playout_device()`] for type-safe device selection.
    ///
    /// # Platform Notes
    ///
    /// **Desktop (Windows, macOS, Linux):** Full device enumeration is supported.
    /// You can enumerate speakers, headphones, USB audio devices, HDMI outputs, etc.
    /// Each device has a unique ID (GUID) and descriptive name.
    ///
    /// **Android:** Only a single "default" device is reported with an empty name and ID.
    /// Android handles audio routing (speaker, earpiece, Bluetooth, wired headset) at the
    /// system level via `AudioManager`, not through WebRTC device selection. To switch
    /// between speaker and earpiece on Android, use the Android `AudioManager` API:
    /// - `audioManager.setSpeakerphoneOn(true/false)`
    /// - `audioManager.setMode(AudioManager.MODE_IN_COMMUNICATION)`
    ///
    /// Device enumeration on Android is not meaningful for user-facing device pickers.
    ///
    /// **iOS:** Similar to desktop - devices can be enumerated and selected, including
    /// built-in speaker, receiver, and connected Bluetooth/wired accessories.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    /// for device in audio.playout_devices() {
    ///     println!("[{}] {} (ID: {})", device.index, device.name, device.id);
    /// }
    ///
    /// // Collect into a Vec for later use
    /// let devices: Vec<_> = audio.playout_devices().collect();
    /// ```
    ///
    /// [`set_playout_device()`]: Self::set_playout_device
    pub fn playout_devices(&self) -> impl Iterator<Item = PlayoutDeviceInfo> + '_ {
        let count = self.playout_device_count();
        (0..count).filter_map(move |index| self.playout_device_info(index))
    }

    fn recording_device_count(&self) -> usize {
        self.handle.runtime.recording_devices() as usize
    }

    fn playout_device_count(&self) -> usize {
        self.handle.runtime.playout_devices() as usize
    }

    fn recording_device_info(&self, index: usize) -> Option<RecordingDeviceInfo> {
        if index >= self.recording_device_count() {
            return None;
        }

        let index = index as u16;
        Some(RecordingDeviceInfo {
            id: RecordingDeviceId::from_unchecked_guid(
                &self.handle.runtime.recording_device_guid(index),
            ),
            name: self.handle.runtime.recording_device_name(index),
            index: index as usize,
        })
    }

    fn playout_device_info(&self, index: usize) -> Option<PlayoutDeviceInfo> {
        if index >= self.playout_device_count() {
            return None;
        }

        let index = index as u16;
        Some(PlayoutDeviceInfo {
            id: PlayoutDeviceId::from_unchecked_guid(
                &self.handle.runtime.playout_device_guid(index),
            ),
            name: self.handle.runtime.playout_device_name(index),
            index: index as usize,
        })
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    fn ensure_device_exists<Device>(
        devices: impl IntoIterator<Item = Device>,
        mut is_match: impl FnMut(&Device) -> bool,
    ) -> AudioResult<()> {
        if devices.into_iter().any(|device| is_match(&device)) {
            Ok(())
        } else {
            Err(AudioError::DeviceNotFound)
        }
    }

    // =========================================================================
    // Device Selection
    // =========================================================================

    /// Selects a recording (microphone) device by ID.
    ///
    /// This is the preferred method for device selection as IDs are stable
    /// across device hot-plug events, unlike indices which can change.
    ///
    /// # Platform Notes
    ///
    /// **Desktop:** Works as expected - select from enumerated devices.
    ///
    /// **Mobile (iOS/Android):** Device selection is a no-op. Both platforms handle
    /// microphone selection at the system level. This method will succeed but has no effect.
    /// - iOS: Apple AudioEngine handles input selection
    /// - Android: System selects best input source based on audio mode
    ///
    /// # Arguments
    ///
    /// * `id` - Device identifier from [`RecordingDeviceInfo::id`]
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::DeviceNotFound`] if the device ID does not exist
    /// in the current list of available recording devices.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    ///
    /// // Get the first microphone (desktop only - on mobile this is a no-op)
    /// if let Some(device) = audio.recording_devices().next() {
    ///     audio.set_recording_device(&device.id)?;
    /// }
    /// ```
    pub fn set_recording_device(&self, id: &RecordingDeviceId) -> AudioResult<()> {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        Self::ensure_device_exists(self.recording_devices(), |device| &device.id == id)?;

        if self.handle.runtime.set_recording_device_by_guid(id.as_str()) {
            Ok(())
        } else {
            Err(AudioError::DeviceNotFound)
        }
    }

    /// Selects a playout (speaker) device by ID.
    ///
    /// This is the preferred method for device selection as IDs are stable
    /// across device hot-plug events, unlike indices which can change.
    ///
    /// # Platform Notes
    ///
    /// **Desktop:** Works as expected - select from enumerated devices.
    ///
    /// **Mobile (iOS/Android):** Device selection is a no-op. Both platforms handle
    /// audio routing at the system level. This method will succeed but has no effect.
    /// - iOS: Use `AVAudioSession` to control routing (speaker, earpiece, Bluetooth)
    /// - Android: Use `AudioManager.setSpeakerphoneOn()` to switch outputs
    ///
    /// # Arguments
    ///
    /// * `id` - Device identifier from [`PlayoutDeviceInfo::id`]
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::DeviceNotFound`] if the device ID does not exist
    /// in the current list of available playout devices.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    ///
    /// // Get the first speaker (desktop only - on mobile this is a no-op)
    /// if let Some(device) = audio.playout_devices().next() {
    ///     audio.set_playout_device(&device.id)?;
    /// }
    /// ```
    pub fn set_playout_device(&self, id: &PlayoutDeviceId) -> AudioResult<()> {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        Self::ensure_device_exists(self.playout_devices(), |device| &device.id == id)?;

        let runtime = &self.handle.runtime;
        if !runtime.set_playout_device_by_guid(id.as_str()) {
            return Err(AudioError::DeviceNotFound);
        }

        // Note: We intentionally do NOT call init_playout()/start_playout() here.
        // On iOS, calling these too early causes a race condition crash in
        // AudioDeviceIOS::OnChangedOutputVolume() because the KVO observers
        // fire before the audio device is fully initialized.
        // WebRTC will automatically initialize and start playout when needed
        // (e.g., when remote audio arrives or when a track is subscribed).

        Ok(())
    }

    /// Switches the recording device while audio is active (hot-swap).
    ///
    /// Unlike [`set_recording_device`], this method handles the stop/change/restart
    /// sequence required when recording is already active.
    ///
    /// # Arguments
    ///
    /// * `id` - Device identifier from [`RecordingDeviceInfo::id`]
    ///
    /// # Errors
    ///
    /// - [`AudioError::DeviceNotFound`] if the device is no longer available
    /// - [`AudioError::OperationFailed`] if any step fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // During an active call, switch to a different microphone
    /// let devices: Vec<_> = audio.recording_devices().collect();
    /// audio.switch_recording_device(&devices[1].id)?;
    /// ```
    ///
    /// [`set_recording_device`]: Self::set_recording_device
    pub fn switch_recording_device(&self, id: &RecordingDeviceId) -> AudioResult<()> {
        let runtime = &self.handle.runtime;
        let was_initialized = runtime.recording_is_initialized();

        if was_initialized {
            if !runtime.stop_recording() {
                return Err(AudioError::OperationFailed("stop_recording failed".to_string()));
            }
        }

        if !runtime.set_recording_device_by_guid(id.as_str()) {
            return Err(AudioError::DeviceNotFound);
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
    /// * `id` - Device identifier from [`PlayoutDeviceInfo::id`]
    ///
    /// # Errors
    ///
    /// - [`AudioError::DeviceNotFound`] if the device is no longer available
    /// - [`AudioError::OperationFailed`] if any step fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // During an active call, switch to a different speaker
    /// let devices: Vec<_> = audio.playout_devices().collect();
    /// audio.switch_playout_device(&devices[1].id)?;
    /// ```
    ///
    /// [`set_playout_device`]: Self::set_playout_device
    pub fn switch_playout_device(&self, id: &PlayoutDeviceId) -> AudioResult<()> {
        let runtime = &self.handle.runtime;
        let was_initialized = runtime.playout_is_initialized();

        if was_initialized {
            if !runtime.stop_playout() {
                return Err(AudioError::OperationFailed("stop_playout failed".to_string()));
            }
        }

        if !runtime.set_playout_device_by_guid(id.as_str()) {
            return Err(AudioError::DeviceNotFound);
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
    /// # Muting
    ///
    /// To mute the microphone, prefer muting the published track (e.g.
    /// `LocalAudioTrack::mute()`) over calling this method. On Apple platforms
    /// the voice engine then mutes the microphone in hardware using the
    /// configured [`MuteMode`]; on other platforms WebRTC stops and restarts
    /// recording automatically. Avoid mixing manual `stop_recording` with track
    /// mute: unmuting a track does not restart manually stopped recording, so
    /// call [`start_recording`] first in that case.
    ///
    /// # Note
    ///
    /// When recording is stopped, any published audio tracks using `RtcAudioSource::Device`
    /// will send silence.
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::OperationFailed`] if recording could not be stopped.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    ///
    /// // Release the microphone while no track is published
    /// audio.stop_recording()?;
    ///
    /// // Resume recording (also resets hardware mic mute to unmuted)
    /// audio.start_recording()?;
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

    /// Sets how the microphone is muted when a published device-audio track
    /// is muted (e.g. `LocalAudioTrack::mute()`).
    ///
    /// # Platform Behavior
    ///
    /// - **iOS/macOS** (Apple AudioEngine ADM): the voice engine mutes the
    ///   microphone in hardware using the configured mode instead of stopping
    ///   recording. See [`MuteMode`] for the tradeoffs of each mode.
    /// - **Other platforms**: returns [`AudioError::Unsupported`]. Muting a
    ///   track stops and restarts recording instead (default WebRTC behavior).
    ///
    /// # Notes
    ///
    /// - Can be called at any time, including before publishing a track.
    /// - Starting recording (publishing a device track, or [`start_recording`])
    ///   always resets the hardware mic mute to unmuted.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audio = PlatformAudio::new()?;
    ///
    /// // Turn off the mic privacy indicator while muted
    /// audio.set_mute_mode(MuteMode::RestartEngine)?;
    ///
    /// // ... publish a device track, then track.mute() applies the mode ...
    /// ```
    ///
    /// [`start_recording`]: Self::start_recording
    pub fn set_mute_mode(&self, mode: MuteMode) -> AudioResult<()> {
        if self.handle.runtime.set_mute_mode(mode.to_raw()) {
            log::info!("PlatformAudio: set mute mode to {:?}", mode);
            Ok(())
        } else {
            Err(AudioError::Unsupported)
        }
    }

    /// Returns the current [`MuteMode`].
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::Unsupported`] on platforms without the Apple
    /// AudioEngine ADM.
    pub fn mute_mode(&self) -> AudioResult<MuteMode> {
        MuteMode::from_raw(self.handle.runtime.mute_mode()).ok_or(AudioError::Unsupported)
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
    /// - **iOS**: Returns `true` when Apple voice processing can provide AEC
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
    /// - **iOS**: Returns `true` when Apple voice processing can provide AGC
    /// - **Android**: Returns `true` on devices with hardware AGC support
    /// - **Desktop**: Returns `false` (hardware AGC not available)
    pub fn is_hardware_agc_available(&self) -> bool {
        self.handle.runtime.builtin_agc_is_available()
    }

    /// Checks if hardware noise suppression is available on this device.
    ///
    /// # Platform Behavior
    ///
    /// - **iOS**: Returns `true` when Apple voice processing can provide NS
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
        let options = *self.handle.processing_options.lock();
        if !options.echo_cancellation {
            AudioProcessingType::None
        } else if options.prefer_hardware_processing && self.is_hardware_aec_available() {
            AudioProcessingType::Hardware
        } else {
            AudioProcessingType::Software
        }
    }

    /// Gets the type of automatic gain control currently active.
    pub fn active_agc_type(&self) -> AudioProcessingType {
        let options = *self.handle.processing_options.lock();
        if !options.auto_gain_control {
            AudioProcessingType::None
        } else if options.prefer_hardware_processing && self.is_hardware_agc_available() {
            AudioProcessingType::Hardware
        } else {
            AudioProcessingType::Software
        }
    }

    /// Gets the type of noise suppression currently active.
    pub fn active_ns_type(&self) -> AudioProcessingType {
        let options = *self.handle.processing_options.lock();
        if !options.noise_suppression {
            AudioProcessingType::None
        } else if options.prefer_hardware_processing && self.is_hardware_ns_available() {
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
    /// - **iOS/macOS**: `prefer_hardware_processing` uses Apple voice processing
    ///   when available (enabled by default)
    /// - **Android**: When `prefer_hardware_processing` is `false`, hardware
    ///   effects are disabled and WebRTC's software APM is used instead
    /// - **Windows/Linux**: `prefer_hardware_processing` is ignored (hardware not available)
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

        *self.handle.processing_options.lock() = options;

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
            .field("recording_device_count", &self.recording_device_count())
            .field("playout_device_count", &self.playout_device_count())
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
