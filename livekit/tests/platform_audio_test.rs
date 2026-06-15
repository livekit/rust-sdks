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

//! Tests for PlatformAudio functionality.
//!
//! These tests verify:
//! - PlatformAudio creation and reference counting
//! - Device enumeration and selection
//! - Audio processing configuration (AEC, AGC, NS)
//! - Integration with room connections
//! - Combining PlatformAudio with NativeAudioSource

mod common;

use std::time::Duration;

use anyhow::Result;
use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
use livekit::{AudioError, AudioResult, PlatformAudio, RtcAudioSource};
use serial_test::serial;

#[cfg(feature = "__lk-e2e-test")]
use anyhow::anyhow;
#[cfg(feature = "__lk-e2e-test")]
use common::test_rooms;
#[cfg(feature = "__lk-e2e-test")]
use livekit::options::TrackPublishOptions;
#[cfg(feature = "__lk-e2e-test")]
use livekit::prelude::{ConnectionState, LocalAudioTrack, LocalTrack, RoomEvent, TrackSource};
#[cfg(feature = "__lk-e2e-test")]
use tokio::time::timeout;

fn try_create_platform_audio(test_name: &str) -> Option<PlatformAudio> {
    match PlatformAudio::new() {
        Ok(audio) => Some(audio),
        Err(err) => {
            log::info!("Skipping {test_name} - PlatformAudio unavailable: {err}");
            None
        }
    }
}

fn try_acquire_platform_adm(
    pcf: &libwebrtc::peer_connection_factory::PeerConnectionFactory,
    test_name: &str,
) -> bool {
    if pcf.acquire_platform_adm() {
        true
    } else {
        log::info!("Skipping {test_name} - Platform ADM unavailable on this environment");
        false
    }
}

// =============================================================================
// Unit Tests (no E2E feature required)
// =============================================================================

#[test]
fn test_audio_error_display() {
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
fn test_audio_error_equality() {
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
fn test_audio_error_clone() {
    let err = AudioError::OperationFailed("test".to_string());
    let cloned = err.clone();
    assert_eq!(err, cloned);
}

#[test]
fn test_audio_result_ok() {
    let result: AudioResult<i32> = Ok(42);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
}

#[test]
fn test_audio_result_err() {
    let result: AudioResult<i32> = Err(AudioError::InvalidDeviceIndex);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), AudioError::InvalidDeviceIndex);
}

#[test]
fn test_rtc_audio_source_device() {
    let source = RtcAudioSource::Device;
    assert!(matches!(source, RtcAudioSource::Device));
}

#[test]
fn test_audio_processing_options_default() {
    use livekit::AudioProcessingOptions;

    let opts = AudioProcessingOptions::default();
    assert!(opts.echo_cancellation);
    assert!(opts.noise_suppression);
    assert!(opts.auto_gain_control);
    assert!(!opts.prefer_hardware_processing); // Default to software (more reliable)
}

#[test]
fn test_audio_processing_options_custom() {
    use livekit::AudioProcessingOptions;

    let opts = AudioProcessingOptions {
        echo_cancellation: false,
        noise_suppression: true,
        auto_gain_control: false,
        prefer_hardware_processing: true,
    };
    assert!(!opts.echo_cancellation);
    assert!(opts.noise_suppression);
    assert!(!opts.auto_gain_control);
    assert!(opts.prefer_hardware_processing);
}

#[test]
fn test_audio_processing_type_default() {
    use livekit::AudioProcessingType;

    let atype = AudioProcessingType::default();
    assert_eq!(atype, AudioProcessingType::Software);
}

#[test]
fn test_audio_processing_type_variants() {
    use livekit::AudioProcessingType;

    let hw = AudioProcessingType::Hardware;
    let sw = AudioProcessingType::Software;
    let none = AudioProcessingType::None;

    assert_ne!(hw, sw);
    assert_ne!(sw, none);
    assert_ne!(hw, none);

    // Test Debug
    assert!(format!("{:?}", hw).contains("Hardware"));
    assert!(format!("{:?}", sw).contains("Software"));
    assert!(format!("{:?}", none).contains("None"));
}

#[test]
fn test_audio_processing_options_clone() {
    use livekit::AudioProcessingOptions;

    let opts = AudioProcessingOptions {
        echo_cancellation: false,
        noise_suppression: true,
        auto_gain_control: false,
        prefer_hardware_processing: true,
    };
    let cloned = opts.clone();
    assert_eq!(opts, cloned);
}

// =============================================================================
// Standalone Tests (no E2E feature required, but require audio hardware)
// =============================================================================

/// Test PlatformAudio creation and basic functionality.
/// This test doesn't require a room connection.
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_standalone_creation() -> Result<()> {
    use livekit::reset_platform_audio;

    // Ensure clean state
    reset_platform_audio();

    // Create PlatformAudio
    let Some(audio) = try_create_platform_audio("test_platform_audio_standalone_creation") else {
        return Ok(());
    };
    log::info!("PlatformAudio created successfully");

    // Check ref count
    assert_eq!(audio.ref_count(), 1);
    log::info!("Initial ref_count: {}", audio.ref_count());

    // Check source type
    assert!(matches!(audio.rtc_source(), RtcAudioSource::Device));
    log::info!("rtc_source() returns Device variant");

    // Enumerate devices using iterators
    let recording_devices: Vec<_> = audio.recording_devices().collect();
    let playout_devices: Vec<_> = audio.playout_devices().collect();
    log::info!(
        "Found {} recording devices, {} playout devices",
        recording_devices.len(),
        playout_devices.len()
    );

    // List recording devices
    for device in &recording_devices {
        log::info!("  Recording device {}: {} (ID: {})", device.index, device.name, device.id);
    }

    // List playout devices
    for device in &playout_devices {
        log::info!("  Playout device {}: {} (ID: {})", device.index, device.name, device.id);
    }

    // Test Debug trait
    let debug_str = format!("{:?}", audio);
    log::info!("Debug output: {}", debug_str);
    assert!(debug_str.contains("PlatformAudio"));

    // Cleanup
    drop(audio);
    log::info!("PlatformAudio dropped successfully");

    Ok(())
}

/// Test PlatformAudio reference counting without room connection.
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_standalone_ref_counting() -> Result<()> {
    use livekit::reset_platform_audio;

    reset_platform_audio();

    // Create first instance
    let Some(audio1) = try_create_platform_audio("test_platform_audio_standalone_ref_counting")
    else {
        return Ok(());
    };
    assert_eq!(audio1.ref_count(), 1);
    log::info!("Created audio1, ref_count: {}", audio1.ref_count());

    // Create second instance - should share ADM
    let audio2 = PlatformAudio::new()?;
    assert_eq!(audio1.ref_count(), 2);
    assert_eq!(audio2.ref_count(), 2);
    log::info!("Created audio2, ref_count: {}", audio1.ref_count());

    // Clone - should increase ref count
    let audio3 = audio1.clone();
    assert_eq!(audio1.ref_count(), 3);
    assert_eq!(audio2.ref_count(), 3);
    assert_eq!(audio3.ref_count(), 3);
    log::info!("Cloned audio1 to audio3, ref_count: {}", audio1.ref_count());

    // Drop one
    drop(audio2);
    assert_eq!(audio1.ref_count(), 2);
    log::info!("Dropped audio2, ref_count: {}", audio1.ref_count());

    // Drop all
    drop(audio1);
    assert_eq!(audio3.ref_count(), 1);
    log::info!("Dropped audio1, audio3 ref_count: {}", audio3.ref_count());

    drop(audio3);
    log::info!("Dropped audio3, all references released");

    Ok(())
}

/// Test device selection without room connection.
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_standalone_device_selection() -> Result<()> {
    use livekit::reset_platform_audio;

    reset_platform_audio();

    let Some(audio) = try_create_platform_audio("test_platform_audio_standalone_device_selection")
    else {
        return Ok(());
    };

    let recording_devices: Vec<_> = audio.recording_devices().collect();
    let playout_devices: Vec<_> = audio.playout_devices().collect();

    // Select recording device if available
    if let Some(device) = recording_devices.first() {
        audio.set_recording_device(&device.id)?;
        log::info!("Selected recording device: {} (ID: {})", device.name, device.id);
    } else {
        log::info!("No recording devices available");
    }

    // Select playout device if available
    if let Some(device) = playout_devices.first() {
        audio.set_playout_device(&device.id)?;
        log::info!("Selected playout device: {} (ID: {})", device.name, device.id);
    } else {
        log::info!("No playout devices available");
    }

    // Note: With the new type-safe API, we can only get device IDs from enumeration.
    // This prevents passing arbitrary/invalid IDs. The only error case is when a
    // device disappears between enumeration and selection (DeviceNotFound).
    log::info!("Type-safe device selection verified");

    drop(audio);
    Ok(())
}

/// Test invalid device IDs return DeviceNotFound.
#[test_log::test(tokio::test)]
#[serial]
#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn test_platform_audio_invalid_device_id_returns_device_not_found() -> Result<()> {
    use livekit::{reset_platform_audio, PlayoutDeviceId, RecordingDeviceId};

    reset_platform_audio();

    let Some(audio) =
        try_create_platform_audio("test_platform_audio_invalid_device_id_returns_device_not_found")
    else {
        return Ok(());
    };

    let invalid_recording_id =
        RecordingDeviceId::from_unchecked_guid("__livekit_missing_recording_device__");
    assert_eq!(audio.set_recording_device(&invalid_recording_id), Err(AudioError::DeviceNotFound));

    let invalid_playout_id =
        PlayoutDeviceId::from_unchecked_guid("__livekit_missing_playout_device__");
    assert_eq!(audio.set_playout_device(&invalid_playout_id), Err(AudioError::DeviceNotFound));

    drop(audio);
    Ok(())
}

/// Test audio processing configuration without room connection.
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_standalone_processing_config() -> Result<()> {
    use livekit::reset_platform_audio;
    use livekit::AudioProcessingOptions;
    use livekit::AudioProcessingType;

    reset_platform_audio();

    let Some(audio) = try_create_platform_audio("test_platform_audio_standalone_processing_config")
    else {
        return Ok(());
    };

    // Query hardware availability
    let hw_aec = audio.is_hardware_aec_available();
    let hw_agc = audio.is_hardware_agc_available();
    let hw_ns = audio.is_hardware_ns_available();
    log::info!("Hardware availability: AEC={}, AGC={}, NS={}", hw_aec, hw_agc, hw_ns);

    // Query active processing types
    let aec_type = audio.active_aec_type();
    let agc_type = audio.active_agc_type();
    let ns_type = audio.active_ns_type();
    log::info!("Active processing: AEC={:?}, AGC={:?}, NS={:?}", aec_type, agc_type, ns_type);

    // Verify consistency
    if hw_aec {
        assert_eq!(aec_type, AudioProcessingType::Hardware);
    } else {
        assert_eq!(aec_type, AudioProcessingType::Software);
    }

    // Configure with default options
    audio.configure_audio_processing(AudioProcessingOptions::default())?;
    log::info!("Configured with default options");

    // Configure with custom options
    audio.configure_audio_processing(AudioProcessingOptions {
        echo_cancellation: true,
        noise_suppression: false,
        auto_gain_control: true,
        prefer_hardware_processing: false,
    })?;
    log::info!("Configured with custom options");

    // Test individual controls
    audio.set_echo_cancellation(true, false)?;
    log::info!("Set AEC: enabled, prefer software");

    audio.set_auto_gain_control(true, false)?;
    log::info!("Set AGC: enabled, prefer software");

    audio.set_noise_suppression(true, false)?;
    log::info!("Set NS: enabled, prefer software");

    drop(audio);
    Ok(())
}

/// Test reset_platform_audio function.
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_standalone_reset() -> Result<()> {
    use livekit::reset_platform_audio;

    reset_platform_audio();

    let Some(audio1) = try_create_platform_audio("test_platform_audio_standalone_reset") else {
        return Ok(());
    };
    assert_eq!(audio1.ref_count(), 1);
    log::info!("Created audio1, ref_count: {}", audio1.ref_count());

    // Force reset (drops internal weak reference, but audio1 still holds strong ref)
    reset_platform_audio();
    log::info!("Called reset_platform_audio()");

    // audio1 still holds a strong reference, so ref_count is still 1
    assert_eq!(audio1.ref_count(), 1);
    log::info!("audio1 ref_count after reset: {}", audio1.ref_count());

    // Create new instance after reset
    // Since weak reference was cleared, audio2 creates a NEW handle
    // audio1 and audio2 now have SEPARATE handles (not shared)
    let audio2 = PlatformAudio::new()?;
    assert_eq!(audio2.ref_count(), 1); // New separate handle
    assert_eq!(audio1.ref_count(), 1); // Original handle unchanged
    log::info!("Created audio2 after reset, audio2.ref_count: {}", audio2.ref_count());
    log::info!("audio1.ref_count still: {}", audio1.ref_count());

    // Now if we create audio3, it should share with audio2 (the new handle)
    let audio3 = PlatformAudio::new()?;
    assert_eq!(audio2.ref_count(), 2); // Shares with audio3
    assert_eq!(audio3.ref_count(), 2);
    assert_eq!(audio1.ref_count(), 1); // Still separate
    log::info!(
        "Created audio3, audio2/3 ref_count: {}, audio1 ref_count: {}",
        audio2.ref_count(),
        audio1.ref_count()
    );

    drop(audio1);
    drop(audio2);
    drop(audio3);

    log::info!("reset_platform_audio test completed");
    Ok(())
}

/// Test PlatformAudio lifecycle - create, use, destroy.
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_standalone_lifecycle() -> Result<()> {
    use livekit::reset_platform_audio;

    reset_platform_audio();

    // Phase 1: Create and configure
    log::info!("=== Phase 1: Create and Configure ===");
    let Some(audio) = try_create_platform_audio("test_platform_audio_standalone_lifecycle") else {
        return Ok(());
    };
    log::info!("Created PlatformAudio");

    let recording_devices: Vec<_> = audio.recording_devices().collect();
    let playout_devices: Vec<_> = audio.playout_devices().collect();
    log::info!("Devices: {} recording, {} playout", recording_devices.len(), playout_devices.len());

    if let Some(device) = recording_devices.first() {
        audio.set_recording_device(&device.id)?;
        log::info!("Set recording device to {}", device.name);
    }
    if let Some(device) = playout_devices.first() {
        audio.set_playout_device(&device.id)?;
        log::info!("Set playout device to {}", device.name);
    }

    // Phase 2: Get audio source (simulating track creation)
    log::info!("=== Phase 2: Get Audio Source ===");
    let source = audio.rtc_source();
    assert!(matches!(source, RtcAudioSource::Device));
    log::info!("Got RtcAudioSource::Device for track creation");

    // Phase 3: Simulate some activity
    log::info!("=== Phase 3: Simulated Activity ===");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    log::info!("Simulated 100ms of activity");

    // Phase 4: Cleanup
    log::info!("=== Phase 4: Cleanup ===");
    audio.release();
    log::info!("Called release(), PlatformAudio destroyed");

    // Phase 5: Verify we can create again
    log::info!("=== Phase 5: Verify Re-creation ===");
    let audio2 = PlatformAudio::new()?;
    assert_eq!(audio2.ref_count(), 1);
    log::info!("Created new PlatformAudio successfully");
    drop(audio2);

    log::info!("Lifecycle test completed successfully");
    Ok(())
}

// =============================================================================
// E2E Tests (require __lk-e2e-test feature)
// =============================================================================

/// Test PlatformAudio creation.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_creation() -> Result<()> {
    use livekit::reset_platform_audio;

    // Ensure clean state
    reset_platform_audio();

    // Create PlatformAudio
    let Some(audio) = try_create_platform_audio("test_platform_audio_creation") else {
        return Ok(());
    };
    assert_eq!(audio.ref_count(), 1);
    assert!(matches!(audio.rtc_source(), RtcAudioSource::Device));

    let recording_devices: Vec<_> = audio.recording_devices().collect();
    let playout_devices: Vec<_> = audio.playout_devices().collect();
    log::info!(
        "PlatformAudio: {} recording devices, {} playout devices",
        recording_devices.len(),
        playout_devices.len()
    );

    // Cleanup
    drop(audio);
    Ok(())
}

/// Test PlatformAudio reference counting.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_ref_counting() -> Result<()> {
    use livekit::reset_platform_audio;

    reset_platform_audio();

    // Create first instance
    let Some(audio1) = try_create_platform_audio("test_platform_audio_ref_counting") else {
        return Ok(());
    };
    assert_eq!(audio1.ref_count(), 1);

    // Create second instance - should share ADM
    let audio2 = PlatformAudio::new()?;
    assert_eq!(audio1.ref_count(), 2);
    assert_eq!(audio2.ref_count(), 2);

    // Clone - should increase ref count
    let audio3 = audio1.clone();
    assert_eq!(audio1.ref_count(), 3);

    // Drop one
    drop(audio2);
    assert_eq!(audio1.ref_count(), 2);

    // Drop all
    drop(audio1);
    drop(audio3);

    log::info!("Reference counting works correctly");
    Ok(())
}

/// Test device enumeration.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_device_enumeration() -> Result<()> {
    use livekit::reset_platform_audio;

    reset_platform_audio();

    let Some(audio) = try_create_platform_audio("test_platform_audio_device_enumeration") else {
        return Ok(());
    };

    // Enumerate recording devices using iterator
    let recording_devices: Vec<_> = audio.recording_devices().collect();
    log::info!("Recording devices: {}", recording_devices.len());
    for device in &recording_devices {
        log::info!("  Mic {}: {} (ID: {})", device.index, device.name, device.id);
    }

    // Enumerate playout devices using iterator
    let playout_devices: Vec<_> = audio.playout_devices().collect();
    log::info!("Playout devices: {}", playout_devices.len());
    for device in &playout_devices {
        log::info!("  Speaker {}: {} (ID: {})", device.index, device.name, device.id);
    }

    drop(audio);
    Ok(())
}

/// Test device selection.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_device_selection() -> Result<()> {
    use livekit::reset_platform_audio;

    reset_platform_audio();

    let Some(audio) = try_create_platform_audio("test_platform_audio_device_selection") else {
        return Ok(());
    };

    let recording_devices: Vec<_> = audio.recording_devices().collect();
    let playout_devices: Vec<_> = audio.playout_devices().collect();

    // Select recording device if available
    if let Some(device) = recording_devices.first() {
        audio.set_recording_device(&device.id)?;
        log::info!("Selected recording device: {}", device.name);
    }

    // Select playout device if available
    if let Some(device) = playout_devices.first() {
        audio.set_playout_device(&device.id)?;
        log::info!("Selected playout device: {}", device.name);
    }

    // Note: With type-safe device IDs, we can't easily test invalid IDs
    // (that's the point of the type safety!)

    drop(audio);
    Ok(())
}

/// Test explicit release.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_release() -> Result<()> {
    use livekit::reset_platform_audio;

    reset_platform_audio();

    let Some(audio) = try_create_platform_audio("test_platform_audio_release") else {
        return Ok(());
    };
    assert_eq!(audio.ref_count(), 1);

    // Explicit release
    audio.release();

    log::info!("Explicit release works");
    Ok(())
}

/// Test combining PlatformAudio with NativeAudioSource.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_with_native_source() -> Result<()> {
    use livekit::reset_platform_audio;
    use livekit::webrtc::audio_source::native::NativeAudioSource;
    use livekit::webrtc::audio_source::AudioSourceOptions;

    reset_platform_audio();

    // Create PlatformAudio for microphone
    let Some(mic) = try_create_platform_audio("test_platform_audio_with_native_source") else {
        return Ok(());
    };
    log::info!("Created PlatformAudio with {} mics", mic.recording_devices().count());

    // Create NativeAudioSource for screen capture / TTS
    let screen_source = NativeAudioSource::new(AudioSourceOptions::default(), 48000, 2, 100);

    // Both can coexist
    let mic_source = mic.rtc_source();
    let screen_rtc_source = RtcAudioSource::Native(screen_source.clone());

    assert!(matches!(mic_source, RtcAudioSource::Device));
    assert!(matches!(screen_rtc_source, RtcAudioSource::Native(_)));

    let recording_count = mic.recording_devices().count();
    log::info!("PlatformAudio ({} mics) and NativeAudioSource can coexist", recording_count);

    drop(mic);
    Ok(())
}

/// Test PlatformAudio with room connection.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_room_connection() -> Result<()> {
    use livekit::reset_platform_audio;

    reset_platform_audio();

    let Some(audio) = try_create_platform_audio("test_platform_audio_room_connection") else {
        return Ok(());
    };
    let recording_devices: Vec<_> = audio.recording_devices().collect();

    log::info!("Connecting to room with {} recording devices", recording_devices.len());

    // Connect to room
    let mut rooms = test_rooms(1).await?;
    let (room, _events) = rooms.pop().unwrap();

    assert_eq!(room.connection_state(), ConnectionState::Connected);

    // Publish track if microphone available
    if let Some(device) = recording_devices.first() {
        audio.set_recording_device(&device.id)?;

        let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
        room.local_participant()
            .publish_track(
                LocalTrack::Audio(track),
                TrackPublishOptions { source: TrackSource::Microphone, ..Default::default() },
            )
            .await?;

        log::info!("Published audio track using PlatformAudio");

        // Verify track is published
        let publications = room.local_participant().track_publications();
        assert!(
            publications.values().any(|p| p.source() == TrackSource::Microphone),
            "Microphone track should be published"
        );
    } else {
        log::info!("Skipping track publish - no microphone available");
    }

    // Disconnect
    room.close().await?;

    // Drop PlatformAudio to release hardware
    drop(audio);

    log::info!("Room connection test completed");
    Ok(())
}

/// Test two participants with PlatformAudio.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_two_participants() -> Result<()> {
    use livekit::reset_platform_audio;

    reset_platform_audio();

    let Some(audio) = try_create_platform_audio("test_platform_audio_two_participants") else {
        return Ok(());
    };
    let recording_devices: Vec<_> = audio.recording_devices().collect();

    if recording_devices.is_empty() {
        log::info!("Skipping two participants test - no microphone available");
        drop(audio);
        return Ok(());
    }

    audio.set_recording_device(&recording_devices[0].id)?;

    // Connect two participants
    let mut rooms = test_rooms(2).await?;
    let (pub_room, _) = rooms.pop().unwrap();
    let (sub_room, mut sub_events) = rooms.pop().unwrap();

    // Publisher creates and publishes audio track
    let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
    pub_room
        .local_participant()
        .publish_track(
            LocalTrack::Audio(track),
            TrackPublishOptions { source: TrackSource::Microphone, ..Default::default() },
        )
        .await?;

    log::info!("Publisher published audio track");

    // Subscriber should receive the track
    let wait_for_track = async {
        while let Some(event) = sub_events.recv().await {
            if let RoomEvent::TrackSubscribed { track: _, publication, participant } = event {
                log::info!(
                    "Subscriber received track from {} ({:?})",
                    participant.identity(),
                    publication.source()
                );
                assert_eq!(publication.source(), TrackSource::Microphone);
                return Ok(());
            }
        }
        Err(anyhow!("Never received track subscription"))
    };

    timeout(Duration::from_secs(10), wait_for_track).await??;

    // Clean up
    pub_room.close().await?;
    sub_room.close().await?;
    drop(audio);

    Ok(())
}

/// Test device hot-switching during active session.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_device_switching() -> Result<()> {
    use livekit::reset_platform_audio;

    reset_platform_audio();

    let Some(audio) = try_create_platform_audio("test_platform_audio_device_switching") else {
        return Ok(());
    };
    let recording_devices: Vec<_> = audio.recording_devices().collect();
    let playout_devices: Vec<_> = audio.playout_devices().collect();

    log::info!(
        "Device switching test: {} recording, {} playout devices",
        recording_devices.len(),
        playout_devices.len()
    );

    // Need at least 2 devices to test switching
    let can_switch_recording = recording_devices.len() >= 2;
    let can_switch_playout = playout_devices.len() >= 2;

    if !can_switch_recording && !can_switch_playout {
        log::info!("Skipping device switching test (need at least 2 devices)");
        drop(audio);
        return Ok(());
    }

    // Connect to room
    let mut rooms = test_rooms(1).await?;
    let (room, _events) = rooms.pop().unwrap();

    // Publish track if microphone available
    if !recording_devices.is_empty() {
        let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
        room.local_participant()
            .publish_track(
                LocalTrack::Audio(track),
                TrackPublishOptions { source: TrackSource::Microphone, ..Default::default() },
            )
            .await?;
    }

    // Switch recording devices
    if can_switch_recording {
        log::info!("Switching recording device to {}", recording_devices[1].name);
        audio.switch_recording_device(&recording_devices[1].id)?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        log::info!("Switching recording device to {}", recording_devices[0].name);
        audio.switch_recording_device(&recording_devices[0].id)?;
    }

    // Switch playout devices
    if can_switch_playout {
        log::info!("Switching playout device to {}", playout_devices[1].name);
        audio.switch_playout_device(&playout_devices[1].id)?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        log::info!("Switching playout device to {}", playout_devices[0].name);
        audio.switch_playout_device(&playout_devices[0].id)?;
    }

    // Clean up
    room.close().await?;
    drop(audio);

    Ok(())
}

/// Test reset_platform_audio function.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_reset_platform_audio() -> Result<()> {
    use livekit::reset_platform_audio;

    reset_platform_audio();

    let Some(audio) = try_create_platform_audio("test_reset_platform_audio") else {
        return Ok(());
    };
    // Device enumeration works (count may be 0 on CI without audio hardware)
    let _recording: Vec<_> = audio.recording_devices().collect();
    let _playout: Vec<_> = audio.playout_devices().collect();

    // Force reset
    reset_platform_audio();

    // Can create new instance after reset
    let audio2 = PlatformAudio::new()?;
    assert_eq!(audio2.ref_count(), 1);

    drop(audio);
    drop(audio2);

    log::info!("reset_platform_audio works correctly");
    Ok(())
}

/// Test hardware audio processing availability queries.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_hardware_availability() -> Result<()> {
    use livekit::reset_platform_audio;
    use livekit::AudioProcessingType;

    reset_platform_audio();

    let Some(audio) = try_create_platform_audio("test_platform_audio_hardware_availability") else {
        return Ok(());
    };

    // Query hardware availability
    let hw_aec = audio.is_hardware_aec_available();
    let hw_agc = audio.is_hardware_agc_available();
    let hw_ns = audio.is_hardware_ns_available();

    log::info!(
        "Hardware audio processing availability: AEC={}, AGC={}, NS={}",
        hw_aec,
        hw_agc,
        hw_ns
    );

    // On desktop (macOS, Windows, Linux), hardware is typically not available
    // On iOS, hardware is always available
    // On Android, it depends on the device

    // Query active types
    let aec_type = audio.active_aec_type();
    let agc_type = audio.active_agc_type();
    let ns_type = audio.active_ns_type();

    log::info!("Active audio processing: AEC={:?}, AGC={:?}, NS={:?}", aec_type, agc_type, ns_type);

    // Verify consistency: if hardware is available, active type should be Hardware
    if hw_aec {
        assert_eq!(aec_type, AudioProcessingType::Hardware);
    } else {
        assert_eq!(aec_type, AudioProcessingType::Software);
    }

    drop(audio);
    Ok(())
}

/// Test audio processing configuration.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_configure_processing() -> Result<()> {
    use livekit::reset_platform_audio;
    use livekit::AudioProcessingOptions;

    reset_platform_audio();

    let Some(audio) = try_create_platform_audio("test_platform_audio_configure_processing") else {
        return Ok(());
    };

    // Configure with defaults
    audio.configure_audio_processing(AudioProcessingOptions::default())?;
    log::info!("Configured with default options");

    // Configure with custom options
    let custom_opts = AudioProcessingOptions {
        echo_cancellation: true,
        noise_suppression: true,
        auto_gain_control: true,
        prefer_hardware_processing: false,
    };
    audio.configure_audio_processing(custom_opts)?;
    log::info!("Configured with custom options (software preferred)");

    // Configure with hardware preference
    let hw_opts = AudioProcessingOptions {
        echo_cancellation: true,
        noise_suppression: true,
        auto_gain_control: true,
        prefer_hardware_processing: true,
    };
    audio.configure_audio_processing(hw_opts)?;
    log::info!("Configured with hardware preference");

    // Disable some features
    let minimal_opts = AudioProcessingOptions {
        echo_cancellation: false,
        noise_suppression: true,
        auto_gain_control: false,
        prefer_hardware_processing: false,
    };
    audio.configure_audio_processing(minimal_opts)?;
    log::info!("Configured with minimal options");

    drop(audio);
    Ok(())
}

/// Test individual audio processing control methods.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_individual_controls() -> Result<()> {
    use livekit::reset_platform_audio;

    reset_platform_audio();

    let Some(audio) = try_create_platform_audio("test_platform_audio_individual_controls") else {
        return Ok(());
    };

    // Test echo cancellation control
    audio.set_echo_cancellation(true, false)?;
    log::info!("AEC enabled (software)");

    audio.set_echo_cancellation(true, true)?;
    log::info!("AEC enabled (hardware preferred)");

    audio.set_echo_cancellation(false, false)?;
    log::info!("AEC disabled");

    // Test auto gain control
    audio.set_auto_gain_control(true, false)?;
    log::info!("AGC enabled (software)");

    audio.set_auto_gain_control(false, false)?;
    log::info!("AGC disabled");

    // Test noise suppression
    audio.set_noise_suppression(true, false)?;
    log::info!("NS enabled (software)");

    audio.set_noise_suppression(false, false)?;
    log::info!("NS disabled");

    drop(audio);
    Ok(())
}

/// Test audio processing with room connection.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_processing_with_room() -> Result<()> {
    use livekit::reset_platform_audio;
    use livekit::AudioProcessingOptions;

    reset_platform_audio();

    let Some(audio) = try_create_platform_audio("test_platform_audio_processing_with_room") else {
        return Ok(());
    };
    let recording_devices: Vec<_> = audio.recording_devices().collect();

    if recording_devices.is_empty() {
        log::info!("Skipping test - no microphone available");
        drop(audio);
        return Ok(());
    }

    // Configure audio processing before connecting
    audio.configure_audio_processing(AudioProcessingOptions {
        echo_cancellation: true,
        noise_suppression: true,
        auto_gain_control: true,
        prefer_hardware_processing: false, // Use reliable software processing
    })?;

    log::info!("Audio processing configured, connecting to room...");

    // Connect to room
    let mut rooms = test_rooms(1).await?;
    let (room, _events) = rooms.pop().unwrap();

    audio.set_recording_device(&recording_devices[0].id)?;

    // Publish track
    let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
    room.local_participant()
        .publish_track(
            LocalTrack::Audio(track),
            TrackPublishOptions { source: TrackSource::Microphone, ..Default::default() },
        )
        .await?;

    log::info!("Published audio track with configured processing");

    // Verify track is published
    let publications = room.local_participant().track_publications();
    assert!(
        publications.values().any(|p| p.source() == TrackSource::Microphone),
        "Microphone track should be published"
    );

    // Reconfigure audio processing while connected
    audio.configure_audio_processing(AudioProcessingOptions {
        echo_cancellation: true,
        noise_suppression: false, // Disable NS
        auto_gain_control: true,
        prefer_hardware_processing: false,
    })?;
    log::info!("Reconfigured audio processing while connected");

    // Clean up
    room.close().await?;
    drop(audio);

    log::info!("Audio processing with room test completed");
    Ok(())
}

// =============================================================================
// ADM Lifecycle and Mode Switching Tests
// =============================================================================
// These tests verify the ADM proxy behavior:
// - Both Dummy ADM and Platform ADM are created at startup
// - Platform ADM ref counting works correctly
// - Mode switching between synthetic and platform modes
// - ADM state consistency

/// Test Platform ADM reference counting through low-level API.
///
/// Verifies that acquire_platform_adm/release_platform_adm properly
/// manage the reference count and is_platform_adm_active state.
#[test_log::test(tokio::test)]
#[serial]
async fn test_adm_proxy_platform_ref_counting() -> Result<()> {
    use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
    use livekit::rtc_engine::lk_runtime::LkRuntime;

    // Get the shared runtime (contains the PeerConnectionFactory with ADM)
    let runtime = LkRuntime::instance();
    let pcf = runtime.pc_factory();

    // Initial state: Platform ADM should not be active (ref_count = 0)
    let initial_ref_count = pcf.platform_adm_ref_count();
    log::info!("Initial platform_adm_ref_count: {}", initial_ref_count);

    // Note: If other tests left state, ref count might be > 0
    // We test relative changes rather than absolute values
    let is_active_before = pcf.is_platform_adm_active();
    log::info!("is_platform_adm_active before acquire: {}", is_active_before);

    // Acquire Platform ADM
    if !try_acquire_platform_adm(pcf, "test_adm_proxy_platform_ref_counting") {
        return Ok(());
    }

    let ref_count_after_acquire = pcf.platform_adm_ref_count();
    assert_eq!(ref_count_after_acquire, initial_ref_count + 1, "ref_count should increment by 1");
    assert!(pcf.is_platform_adm_active(), "Platform ADM should be active after acquire");
    log::info!(
        "After acquire: ref_count={}, is_active={}",
        ref_count_after_acquire,
        pcf.is_platform_adm_active()
    );

    // Acquire again (should just increment ref count)
    let acquired2 = pcf.acquire_platform_adm();
    assert!(acquired2, "second acquire_platform_adm should succeed");

    let ref_count_after_second_acquire = pcf.platform_adm_ref_count();
    assert_eq!(
        ref_count_after_second_acquire,
        initial_ref_count + 2,
        "ref_count should be initial + 2"
    );
    log::info!("After second acquire: ref_count={}", ref_count_after_second_acquire);

    // Release once
    pcf.release_platform_adm();
    let ref_count_after_release = pcf.platform_adm_ref_count();
    assert_eq!(
        ref_count_after_release,
        initial_ref_count + 1,
        "ref_count should be initial + 1 after one release"
    );
    assert!(pcf.is_platform_adm_active(), "Platform ADM should still be active (ref_count > 0)");
    log::info!("After first release: ref_count={}", ref_count_after_release);

    // Release again (should return to initial state)
    pcf.release_platform_adm();
    let final_ref_count = pcf.platform_adm_ref_count();
    assert_eq!(final_ref_count, initial_ref_count, "ref_count should return to initial value");
    log::info!(
        "After second release: ref_count={}, is_active={}",
        final_ref_count,
        pcf.is_platform_adm_active()
    );

    // Verify is_platform_adm_active consistency
    if initial_ref_count == 0 {
        assert!(
            !pcf.is_platform_adm_active(),
            "Platform ADM should not be active when ref_count = 0"
        );
    }

    log::info!("ADM proxy platform ref counting test passed");
    Ok(())
}

/// Test ADM recording enabled flag.
///
/// Verifies that set_adm_recording_enabled/adm_recording_enabled
/// properly control the recording mode.
#[test_log::test(tokio::test)]
#[serial]
async fn test_adm_proxy_recording_enabled_flag() -> Result<()> {
    use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
    use livekit::rtc_engine::lk_runtime::LkRuntime;

    let runtime = LkRuntime::instance();
    let pcf = runtime.pc_factory();

    // Save initial state
    let initial_enabled = pcf.adm_recording_enabled();
    log::info!("Initial adm_recording_enabled: {}", initial_enabled);

    // Disable recording
    pcf.set_adm_recording_enabled(false);
    assert!(!pcf.adm_recording_enabled(), "Recording should be disabled");
    log::info!("After set_adm_recording_enabled(false): {}", pcf.adm_recording_enabled());

    // Enable recording
    pcf.set_adm_recording_enabled(true);
    assert!(pcf.adm_recording_enabled(), "Recording should be enabled");
    log::info!("After set_adm_recording_enabled(true): {}", pcf.adm_recording_enabled());

    // Toggle multiple times
    pcf.set_adm_recording_enabled(false);
    pcf.set_adm_recording_enabled(true);
    pcf.set_adm_recording_enabled(false);
    assert!(!pcf.adm_recording_enabled(), "Recording should be disabled after toggles");

    // Restore initial state
    pcf.set_adm_recording_enabled(initial_enabled);
    assert_eq!(
        pcf.adm_recording_enabled(),
        initial_enabled,
        "Recording should be restored to initial state"
    );

    log::info!("ADM recording enabled flag test passed");
    Ok(())
}

/// Test ADM playout enabled flag.
///
/// Verifies that set_adm_playout_enabled/adm_playout_enabled
/// properly control the playout mode (synthetic vs platform).
#[test_log::test(tokio::test)]
#[serial]
async fn test_adm_proxy_playout_enabled_flag() -> Result<()> {
    use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
    use livekit::rtc_engine::lk_runtime::LkRuntime;

    let runtime = LkRuntime::instance();
    let pcf = runtime.pc_factory();

    // Save initial state
    let initial_enabled = pcf.adm_playout_enabled();
    log::info!("Initial adm_playout_enabled: {}", initial_enabled);

    // Disable playout (use synthetic mode / Dummy ADM)
    pcf.set_adm_playout_enabled(false);
    assert!(!pcf.adm_playout_enabled(), "Playout should be disabled (synthetic mode)");
    log::info!("After set_adm_playout_enabled(false): {}", pcf.adm_playout_enabled());

    // Enable playout (use Platform ADM speakers)
    pcf.set_adm_playout_enabled(true);
    assert!(pcf.adm_playout_enabled(), "Playout should be enabled (platform mode)");
    log::info!("After set_adm_playout_enabled(true): {}", pcf.adm_playout_enabled());

    // Toggle multiple times
    pcf.set_adm_playout_enabled(false);
    pcf.set_adm_playout_enabled(true);
    pcf.set_adm_playout_enabled(false);
    assert!(!pcf.adm_playout_enabled(), "Playout should be disabled after toggles");

    // Restore initial state
    pcf.set_adm_playout_enabled(initial_enabled);
    assert_eq!(
        pcf.adm_playout_enabled(),
        initial_enabled,
        "Playout should be restored to initial state"
    );

    log::info!("ADM playout enabled flag test passed");
    Ok(())
}

/// Test ADM mode switching while playout is active.
///
/// This tests the SwitchPlayoutAdmIfNeeded() logic by:
/// 1. Starting playout in synthetic mode (Dummy ADM)
/// 2. Enabling platform playout (switches to Platform ADM)
/// 3. Disabling platform playout (switches back to Dummy ADM)
///
/// Note: On some environments without audio hardware, init_playout may fail.
/// The test still verifies that mode switching doesn't crash.
#[test_log::test(tokio::test)]
#[serial]
async fn test_adm_proxy_playout_mode_switching() -> Result<()> {
    use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
    use livekit::rtc_engine::lk_runtime::LkRuntime;

    let runtime = LkRuntime::instance();
    let pcf = runtime.pc_factory();

    // Save initial states
    let initial_playout_enabled = pcf.adm_playout_enabled();
    let initial_ref_count = pcf.platform_adm_ref_count();

    log::info!("=== Phase 1: Setup - Acquire Platform ADM first ===");
    // Acquire Platform ADM first (required for playout to work)
    if !try_acquire_platform_adm(pcf, "test_adm_proxy_playout_mode_switching") {
        return Ok(());
    }
    assert!(pcf.is_platform_adm_active());
    log::info!("Platform ADM acquired");

    // Start in platform playout mode (more likely to succeed with real hardware)
    pcf.set_adm_playout_enabled(true);
    assert!(pcf.adm_playout_enabled());

    // Try to initialize playout
    let init_result = pcf.init_playout();
    log::info!("init_playout() result: {}", init_result);

    let playout_available = init_result;
    if playout_available {
        let start_result = pcf.start_playout();
        log::info!("start_playout() result: {}", start_result);
        log::info!("playout_is_initialized: {}", pcf.playout_is_initialized());
    } else {
        log::info!("Playout not available on this environment, testing mode switches without active playout");
    }

    log::info!("=== Phase 2: Switch to synthetic mode ===");
    // Disable platform playout - this should trigger SwitchPlayoutAdmIfNeeded()
    pcf.set_adm_playout_enabled(false);
    assert!(!pcf.adm_playout_enabled());
    log::info!("Switched to synthetic playout mode (Dummy ADM)");

    // Give some time for the switch to complete
    tokio::time::sleep(Duration::from_millis(50)).await;
    log::info!(
        "playout_is_initialized after switch to synthetic: {}",
        pcf.playout_is_initialized()
    );

    log::info!("=== Phase 3: Switch back to platform mode ===");
    // Enable platform playout - this should trigger SwitchPlayoutAdmIfNeeded()
    pcf.set_adm_playout_enabled(true);
    assert!(pcf.adm_playout_enabled());
    log::info!("Switched back to platform playout mode");

    // Give some time for the switch to complete
    tokio::time::sleep(Duration::from_millis(50)).await;
    log::info!("playout_is_initialized after switch to platform: {}", pcf.playout_is_initialized());

    log::info!("=== Phase 4: Cleanup ===");
    // Stop playout
    pcf.stop_playout();
    log::info!("Stopped playout");

    // Release Platform ADM
    pcf.release_platform_adm();

    // Restore initial states
    pcf.set_adm_playout_enabled(initial_playout_enabled);
    // Release any extra refs we might have acquired
    while pcf.platform_adm_ref_count() > initial_ref_count {
        pcf.release_platform_adm();
    }

    log::info!("ADM playout mode switching test passed - no crashes during mode switches");
    Ok(())
}

/// Test ADM mode switching while recording is active.
///
/// This tests the SwitchRecordingAdmIfNeeded() logic by:
/// 1. Starting in synthetic mode (recording not available without Platform ADM)
/// 2. Acquiring Platform ADM and enabling recording
/// 3. Disabling recording (recording stops)
#[test_log::test(tokio::test)]
#[serial]
async fn test_adm_proxy_recording_mode_switching() -> Result<()> {
    use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
    use livekit::rtc_engine::lk_runtime::LkRuntime;

    let runtime = LkRuntime::instance();
    let pcf = runtime.pc_factory();

    // Save initial states
    let initial_recording_enabled = pcf.adm_recording_enabled();
    let initial_ref_count = pcf.platform_adm_ref_count();

    log::info!("=== Phase 1: Setup - no Platform ADM ===");
    // Ensure recording is disabled initially
    pcf.set_adm_recording_enabled(false);

    // In synthetic mode without Platform ADM, recording is not available
    // (Dummy ADM has no microphone)
    log::info!("recording_is_initialized (no platform ADM): {}", pcf.recording_is_initialized());

    log::info!("=== Phase 2: Acquire Platform ADM and enable recording ===");
    // Acquire Platform ADM
    if !try_acquire_platform_adm(pcf, "test_adm_proxy_recording_mode_switching") {
        return Ok(());
    }
    assert!(pcf.is_platform_adm_active());

    // Enable recording
    pcf.set_adm_recording_enabled(true);
    assert!(pcf.adm_recording_enabled());

    // Now we should be able to init and start recording
    let init_result = pcf.init_recording();
    log::info!("init_recording() result: {}", init_result);

    // Only start if init succeeded and we have recording devices
    if init_result && pcf.recording_devices() > 0 {
        let start_result = pcf.start_recording();
        log::info!("start_recording() result: {}", start_result);

        assert!(pcf.recording_is_initialized(), "Recording should be initialized");
        log::info!("Recording is active on Platform ADM");

        log::info!("=== Phase 3: Disable recording - triggers mode switch ===");
        // Disable recording - this should trigger SwitchRecordingAdmIfNeeded()
        pcf.set_adm_recording_enabled(false);
        assert!(!pcf.adm_recording_enabled());

        // Give some time for the switch
        tokio::time::sleep(Duration::from_millis(50)).await;

        log::info!("recording_is_initialized after disable: {}", pcf.recording_is_initialized());

        // Stop recording
        pcf.stop_recording();
    } else {
        log::info!("Skipping recording test - no recording devices or init failed");
    }

    log::info!("=== Phase 4: Cleanup ===");
    // Release Platform ADM
    pcf.release_platform_adm();

    // Restore initial states
    pcf.set_adm_recording_enabled(initial_recording_enabled);
    while pcf.platform_adm_ref_count() > initial_ref_count {
        pcf.release_platform_adm();
    }

    log::info!("ADM recording mode switching test passed");
    Ok(())
}

/// Test that PlatformAudio properly manages ADM lifecycle.
///
/// PlatformAudio instances share a single underlying handle via Arc.
/// The Platform ADM is acquired when the first instance is created and
/// released when all instances are dropped.
///
/// This test verifies:
/// 1. Platform ADM becomes active when first PlatformAudio is created
/// 2. Platform ADM remains active while any PlatformAudio exists
/// 3. Platform ADM is released when all PlatformAudio instances are dropped
/// 4. Multiple PlatformAudio instances share the same handle (ref_count)
///
/// Note: This test requires audio hardware. On CI environments without audio
/// devices, the test will be skipped gracefully.
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_audio_adm_lifecycle() -> Result<()> {
    use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
    use livekit::reset_platform_audio;
    use livekit::rtc_engine::lk_runtime::LkRuntime;

    reset_platform_audio();

    let runtime = LkRuntime::instance();
    let pcf = runtime.pc_factory();

    log::info!("=== Initial state (no PlatformAudio) ===");
    let initial_cpp_ref_count = pcf.platform_adm_ref_count();
    let initial_is_active = pcf.is_platform_adm_active();
    log::info!("Initial C++ ref_count={}, is_active={}", initial_cpp_ref_count, initial_is_active);

    // After reset_platform_audio(), ref_count should be 0
    if initial_cpp_ref_count == 0 {
        assert!(!initial_is_active, "Platform ADM should not be active when ref_count is 0");
    }

    log::info!("=== Create first PlatformAudio ===");
    let audio1 = match PlatformAudio::new() {
        Ok(audio) => audio,
        Err(e) => {
            log::info!("Skipping test - PlatformAudio::new() failed (no audio devices?): {}", e);
            return Ok(());
        }
    };
    let rust_ref_count_1 = audio1.ref_count();

    let cpp_ref_count_after_first = pcf.platform_adm_ref_count();
    let is_active_after_first = pcf.is_platform_adm_active();
    log::info!(
        "After first create: C++ ref_count={}, is_active={}, Rust ref_count={}",
        cpp_ref_count_after_first,
        is_active_after_first,
        rust_ref_count_1
    );

    // PlatformAudio should have acquired the Platform ADM
    assert!(
        cpp_ref_count_after_first > initial_cpp_ref_count,
        "PlatformAudio should increment C++ platform ADM ref count"
    );
    assert!(is_active_after_first, "Platform ADM should be active after PlatformAudio creation");
    assert_eq!(rust_ref_count_1, 1, "First PlatformAudio Rust ref_count should be 1");

    log::info!("=== Create second PlatformAudio (shares handle with first) ===");
    let audio2 = PlatformAudio::new().expect("Second PlatformAudio should succeed if first did");
    let rust_ref_count_2 = audio2.ref_count();

    // Multiple PlatformAudio instances share the same underlying handle
    // So the C++ ref count may or may not increase (depends on implementation)
    // But the Rust ref count definitely increases
    log::info!(
        "After second create: C++ ref_count={}, Rust ref_count={}",
        pcf.platform_adm_ref_count(),
        rust_ref_count_2
    );
    assert_eq!(rust_ref_count_2, 2, "Second PlatformAudio should share handle (Rust ref_count=2)");
    assert_eq!(audio1.ref_count(), 2, "First audio should also see Rust ref_count=2");

    log::info!("=== Drop first PlatformAudio (second still holds reference) ===");
    drop(audio1);

    // Platform ADM should still be active because audio2 still exists
    assert!(
        pcf.is_platform_adm_active(),
        "Platform ADM should still be active (audio2 still exists)"
    );
    assert_eq!(audio2.ref_count(), 1, "After dropping audio1, Rust ref_count should be 1");
    log::info!(
        "After drop first: C++ ref_count={}, Rust ref_count={}",
        pcf.platform_adm_ref_count(),
        audio2.ref_count()
    );

    log::info!("=== Drop second PlatformAudio (should release Platform ADM) ===");
    drop(audio2);

    let final_cpp_ref_count = pcf.platform_adm_ref_count();
    let final_is_active = pcf.is_platform_adm_active();
    log::info!(
        "After drop second: C++ ref_count={}, is_active={}",
        final_cpp_ref_count,
        final_is_active
    );

    // After all PlatformAudio instances are dropped, C++ ref count should return to initial
    assert_eq!(
        final_cpp_ref_count, initial_cpp_ref_count,
        "C++ ref count should return to initial value after all PlatformAudio dropped"
    );
    if initial_cpp_ref_count == 0 {
        assert!(
            !final_is_active,
            "Platform ADM should not be active when all PlatformAudio instances are dropped"
        );
    }

    log::info!("PlatformAudio ADM lifecycle test passed");
    Ok(())
}

/// Test ADM state consistency under rapid mode changes.
///
/// This stress tests the mode switching logic by rapidly toggling
/// playout and recording modes to catch any race conditions or state
/// inconsistencies.
#[test_log::test(tokio::test)]
#[serial]
async fn test_adm_proxy_rapid_mode_changes() -> Result<()> {
    use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
    use livekit::rtc_engine::lk_runtime::LkRuntime;

    let runtime = LkRuntime::instance();
    let pcf = runtime.pc_factory();

    // Save initial states
    let initial_playout_enabled = pcf.adm_playout_enabled();
    let initial_recording_enabled = pcf.adm_recording_enabled();
    let initial_ref_count = pcf.platform_adm_ref_count();

    log::info!("=== Setup: Acquire Platform ADM ===");
    if !try_acquire_platform_adm(pcf, "test_adm_proxy_rapid_mode_changes") {
        return Ok(());
    }

    // Initialize playout so mode switches actually do something
    pcf.init_playout();
    pcf.start_playout();

    log::info!("=== Rapid playout mode changes ===");
    for i in 0..10 {
        pcf.set_adm_playout_enabled(i % 2 == 0);
        // No sleep - test rapid changes
    }
    log::info!("Completed 10 rapid playout mode changes");

    log::info!("=== Rapid recording mode changes ===");
    for i in 0..10 {
        pcf.set_adm_recording_enabled(i % 2 == 0);
        // No sleep - test rapid changes
    }
    log::info!("Completed 10 rapid recording mode changes");

    log::info!("=== Rapid acquire/release cycles ===");
    for _ in 0..5 {
        pcf.acquire_platform_adm();
        pcf.release_platform_adm();
    }
    log::info!("Completed 5 rapid acquire/release cycles");

    // Verify state consistency
    let current_ref_count = pcf.platform_adm_ref_count();
    let is_active = pcf.is_platform_adm_active();

    // We acquired once at the start and did balanced acquire/release cycles
    assert_eq!(current_ref_count, initial_ref_count + 1, "Ref count should be initial + 1");
    assert!(is_active, "Platform ADM should be active");

    log::info!("=== Cleanup ===");
    pcf.stop_playout();
    pcf.release_platform_adm();

    // Restore initial states
    pcf.set_adm_playout_enabled(initial_playout_enabled);
    pcf.set_adm_recording_enabled(initial_recording_enabled);
    while pcf.platform_adm_ref_count() > initial_ref_count {
        pcf.release_platform_adm();
    }

    log::info!("ADM rapid mode changes test passed - no crashes or state corruption");
    Ok(())
}
