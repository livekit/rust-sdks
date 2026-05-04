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

use anyhow::{anyhow, Result};
#[cfg(feature = "__lk-e2e-test")]
use livekit::options::TrackPublishOptions;
use livekit::{prelude::*, AudioError, AudioResult, PlatformAudio, RtcAudioSource};
use serial_test::serial;
use tokio::time::timeout;

#[cfg(feature = "__lk-e2e-test")]
use common::test_rooms;

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
    let audio = PlatformAudio::new()?;
    log::info!("PlatformAudio created successfully");

    // Check ref count
    assert_eq!(audio.ref_count(), 1);
    log::info!("Initial ref_count: {}", audio.ref_count());

    // Check source type
    assert!(matches!(audio.rtc_source(), RtcAudioSource::Device));
    log::info!("rtc_source() returns Device variant");

    // Enumerate devices
    let recording_count = audio.recording_devices();
    let playout_count = audio.playout_devices();
    log::info!("Found {} recording devices, {} playout devices", recording_count, playout_count);

    // List recording devices
    for i in 0..recording_count as u16 {
        let name = audio.recording_device_name(i);
        log::info!("  Recording device {}: {}", i, name);
    }

    // List playout devices
    for i in 0..playout_count as u16 {
        let name = audio.playout_device_name(i);
        log::info!("  Playout device {}: {}", i, name);
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
    let audio1 = PlatformAudio::new()?;
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

    let audio = PlatformAudio::new()?;

    let recording_count = audio.recording_devices();
    let playout_count = audio.playout_devices();

    // Select recording device if available
    if recording_count > 0 {
        audio.set_recording_device(0)?;
        log::info!("Selected recording device 0: {}", audio.recording_device_name(0));
    } else {
        log::info!("No recording devices available");
    }

    // Select playout device if available
    if playout_count > 0 {
        audio.set_playout_device(0)?;
        log::info!("Selected playout device 0: {}", audio.playout_device_name(0));
    } else {
        log::info!("No playout devices available");
    }

    // Test invalid device index
    let result = audio.set_recording_device(9999);
    assert!(matches!(result, Err(AudioError::InvalidDeviceIndex)));
    log::info!("Invalid recording device index correctly rejected");

    let result = audio.set_playout_device(9999);
    assert!(matches!(result, Err(AudioError::InvalidDeviceIndex)));
    log::info!("Invalid playout device index correctly rejected");

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

    let audio = PlatformAudio::new()?;

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

    let audio1 = PlatformAudio::new()?;
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
    let audio = PlatformAudio::new()?;
    log::info!("Created PlatformAudio");

    let recording_count = audio.recording_devices();
    let playout_count = audio.playout_devices();
    log::info!("Devices: {} recording, {} playout", recording_count, playout_count);

    if recording_count > 0 {
        audio.set_recording_device(0)?;
        log::info!("Set recording device to 0");
    }
    if playout_count > 0 {
        audio.set_playout_device(0)?;
        log::info!("Set playout device to 0");
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
    let audio = PlatformAudio::new()?;
    assert_eq!(audio.ref_count(), 1);
    assert!(matches!(audio.rtc_source(), RtcAudioSource::Device));

    let recording_count = audio.recording_devices();
    let playout_count = audio.playout_devices();
    log::info!(
        "PlatformAudio: {} recording devices, {} playout devices",
        recording_count,
        playout_count
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
    let audio1 = PlatformAudio::new()?;
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

    let audio = PlatformAudio::new()?;

    // Enumerate recording devices
    let recording_count = audio.recording_devices();
    log::info!("Recording devices: {}", recording_count);
    for i in 0..recording_count as u16 {
        let name = audio.recording_device_name(i);
        log::info!("  Mic {}: {}", i, name);
    }

    // Enumerate playout devices
    let playout_count = audio.playout_devices();
    log::info!("Playout devices: {}", playout_count);
    for i in 0..playout_count as u16 {
        let name = audio.playout_device_name(i);
        log::info!("  Speaker {}: {}", i, name);
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

    let audio = PlatformAudio::new()?;

    let recording_count = audio.recording_devices();
    let playout_count = audio.playout_devices();

    // Select recording device if available
    if recording_count > 0 {
        audio.set_recording_device(0)?;
        log::info!("Selected recording device 0");
    }

    // Select playout device if available
    if playout_count > 0 {
        audio.set_playout_device(0)?;
        log::info!("Selected playout device 0");
    }

    // Test invalid device index
    let result = audio.set_recording_device(9999);
    assert!(matches!(result, Err(AudioError::InvalidDeviceIndex)));

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

    let audio = PlatformAudio::new()?;
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
    let mic = PlatformAudio::new()?;
    log::info!("Created PlatformAudio with {} mics", mic.recording_devices());

    // Create NativeAudioSource for screen capture / TTS
    let screen_source = NativeAudioSource::new(AudioSourceOptions::default(), 48000, 2, 100);

    // Both can coexist
    let mic_source = mic.rtc_source();
    let screen_rtc_source = RtcAudioSource::Native(screen_source.clone());

    assert!(matches!(mic_source, RtcAudioSource::Device));
    assert!(matches!(screen_rtc_source, RtcAudioSource::Native(_)));

    log::info!("PlatformAudio and NativeAudioSource can coexist");

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

    let audio = PlatformAudio::new()?;
    let recording_count = audio.recording_devices();

    log::info!("Connecting to room with {} recording devices", recording_count);

    // Connect to room
    let mut rooms = test_rooms(1).await?;
    let (room, _events) = rooms.pop().unwrap();

    assert_eq!(room.connection_state(), ConnectionState::Connected);

    // Publish track if microphone available
    if recording_count > 0 {
        audio.set_recording_device(0)?;

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

    let audio = PlatformAudio::new()?;
    let recording_count = audio.recording_devices();

    if recording_count == 0 {
        log::info!("Skipping two participants test - no microphone available");
        drop(audio);
        return Ok(());
    }

    audio.set_recording_device(0)?;

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

    let audio = PlatformAudio::new()?;
    let recording_count = audio.recording_devices() as u16;
    let playout_count = audio.playout_devices() as u16;

    log::info!(
        "Device switching test: {} recording, {} playout devices",
        recording_count,
        playout_count
    );

    // Need at least 2 devices to test switching
    let can_switch_recording = recording_count >= 2;
    let can_switch_playout = playout_count >= 2;

    if !can_switch_recording && !can_switch_playout {
        log::info!("Skipping device switching test (need at least 2 devices)");
        drop(audio);
        return Ok(());
    }

    // Connect to room
    let mut rooms = test_rooms(1).await?;
    let (room, _events) = rooms.pop().unwrap();

    // Publish track if microphone available
    if recording_count > 0 {
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
        log::info!("Switching recording device 0 -> 1");
        audio.switch_recording_device(1)?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        log::info!("Switching recording device 1 -> 0");
        audio.switch_recording_device(0)?;
    }

    // Switch playout devices
    if can_switch_playout {
        log::info!("Switching playout device 0 -> 1");
        audio.switch_playout_device(1)?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        log::info!("Switching playout device 1 -> 0");
        audio.switch_playout_device(0)?;
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

    let audio = PlatformAudio::new()?;
    assert!(audio.recording_devices() >= 0 || audio.playout_devices() >= 0);

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

    let audio = PlatformAudio::new()?;

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

    let audio = PlatformAudio::new()?;

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

    let audio = PlatformAudio::new()?;

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

    let audio = PlatformAudio::new()?;
    let recording_count = audio.recording_devices();

    if recording_count == 0 {
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

    audio.set_recording_device(0)?;

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
