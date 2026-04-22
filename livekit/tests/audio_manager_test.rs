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

//! Tests for AudioManager and audio device management.
//!
//! Unit tests run without a LiveKit server and test the AudioManager API.
//! Integration tests (with __lk-e2e-test feature) test full audio flow with a server.
//!
//! Note: Tests that modify AudioManager state use `#[serial]` to prevent
//! interference since AudioManager is a global singleton.

use livekit::{AudioError, AudioManager, AudioMode};
use libwebrtc::native::AdmDelegateType;
use serial_test::serial;

mod common;

// ============================================================================
// Unit Tests - No server required, run on CI
// ============================================================================

/// Test that AudioManager::instance() returns a valid instance.
#[test]
fn test_audio_manager_instance() {
    let audio = AudioManager::instance();

    // Should be able to get debug info
    let debug_str = format!("{:?}", audio);
    assert!(debug_str.contains("AudioManager"));
}

/// Test that multiple calls to instance() return equivalent managers.
#[test]
fn test_audio_manager_singleton() {
    let audio1 = AudioManager::instance();
    let audio2 = AudioManager::instance();

    // Both should report the same mode
    assert_eq!(audio1.current_mode(), audio2.current_mode());
}

/// Test default mode is Synthetic.
#[test]
#[serial]
fn test_default_mode_is_synthetic() {
    let audio = AudioManager::instance();

    // Reset to ensure clean state
    audio.reset();

    // Default should be Synthetic
    assert_eq!(audio.current_mode(), AdmDelegateType::Synthetic);
}

/// Test setting Synthetic mode explicitly.
#[test]
#[serial]
fn test_set_synthetic_mode() {
    let audio = AudioManager::instance();

    // Set to Synthetic mode
    let result = audio.set_mode(AudioMode::Synthetic);
    assert!(result.is_ok());

    assert_eq!(audio.current_mode(), AdmDelegateType::Synthetic);
}

/// Test setting Platform mode.
#[test]
#[serial]
fn test_set_platform_mode() {
    let audio = AudioManager::instance();

    // Set to Platform mode
    let result = audio.set_mode(AudioMode::Platform);

    // Platform mode may fail if no audio devices are available (CI environment)
    // So we check either success or PlatformAdmInitFailed
    match result {
        Ok(()) => {
            assert_eq!(audio.current_mode(), AdmDelegateType::Platform);
            // Clean up
            audio.reset();
        }
        Err(AudioError::PlatformAdmInitFailed) => {
            // This is acceptable on CI without audio hardware
            log::info!("Platform ADM init failed (expected on CI without audio hardware)");
        }
        Err(e) => {
            panic!("Unexpected error: {:?}", e);
        }
    }
}

/// Test mode switching from Synthetic to Platform and back.
#[test]
#[serial]
fn test_mode_switching() {
    let audio = AudioManager::instance();

    // Start in Synthetic mode
    audio.reset();
    assert_eq!(audio.current_mode(), AdmDelegateType::Synthetic);

    // Try to switch to Platform
    if audio.set_mode(AudioMode::Platform).is_ok() {
        assert_eq!(audio.current_mode(), AdmDelegateType::Platform);

        // Switch back to Synthetic
        audio.set_mode(AudioMode::Synthetic).unwrap();
        assert_eq!(audio.current_mode(), AdmDelegateType::Synthetic);
    }
}

/// Test multiple mode switches back and forth.
/// Verifies that mode can be switched multiple times before connecting to a room.
#[test]
#[serial]
fn test_multiple_mode_switches() {
    let audio = AudioManager::instance();

    // Start fresh
    audio.reset();
    assert_eq!(audio.current_mode(), AdmDelegateType::Synthetic);

    // Skip if Platform mode is not available (no audio hardware)
    if audio.set_mode(AudioMode::Platform).is_err() {
        log::info!("Skipping multiple mode switches test (no audio hardware)");
        return;
    }

    // Verify Platform mode
    assert_eq!(audio.current_mode(), AdmDelegateType::Platform);
    let platform_recording_count = audio.recording_devices();
    let platform_playout_count = audio.playout_devices();
    log::info!(
        "Platform mode: {} recording, {} playout devices",
        platform_recording_count,
        platform_playout_count
    );

    // Switch back to Synthetic
    audio.set_mode(AudioMode::Synthetic).unwrap();
    assert_eq!(audio.current_mode(), AdmDelegateType::Synthetic);
    assert_eq!(audio.recording_devices(), 0, "Synthetic mode should have 0 recording devices");
    assert_eq!(audio.playout_devices(), 0, "Synthetic mode should have 0 playout devices");

    // Switch to Platform again
    audio.set_mode(AudioMode::Platform).unwrap();
    assert_eq!(audio.current_mode(), AdmDelegateType::Platform);
    assert_eq!(audio.recording_devices(), platform_recording_count);
    assert_eq!(audio.playout_devices(), platform_playout_count);

    // Switch back to Synthetic again
    audio.set_mode(AudioMode::Synthetic).unwrap();
    assert_eq!(audio.current_mode(), AdmDelegateType::Synthetic);
    assert_eq!(audio.recording_devices(), 0);
    assert_eq!(audio.playout_devices(), 0);

    // One more round trip
    audio.set_mode(AudioMode::Platform).unwrap();
    assert_eq!(audio.current_mode(), AdmDelegateType::Platform);

    audio.set_mode(AudioMode::Synthetic).unwrap();
    assert_eq!(audio.current_mode(), AdmDelegateType::Synthetic);

    log::info!("Successfully completed 3 round-trip mode switches");
}

/// Test that setting the same mode twice is idempotent.
#[test]
#[serial]
fn test_mode_switch_idempotent() {
    let audio = AudioManager::instance();

    // Start fresh
    audio.reset();

    // Setting Synthetic when already in Synthetic should be OK
    audio.set_mode(AudioMode::Synthetic).unwrap();
    assert_eq!(audio.current_mode(), AdmDelegateType::Synthetic);
    audio.set_mode(AudioMode::Synthetic).unwrap();
    assert_eq!(audio.current_mode(), AdmDelegateType::Synthetic);

    // Try Platform mode
    if audio.set_mode(AudioMode::Platform).is_ok() {
        assert_eq!(audio.current_mode(), AdmDelegateType::Platform);

        // Setting Platform when already in Platform should be OK
        audio.set_mode(AudioMode::Platform).unwrap();
        assert_eq!(audio.current_mode(), AdmDelegateType::Platform);

        audio.reset();
    }
}

/// Test that device selection persists across mode switches within Platform mode,
/// but is cleared when switching to Synthetic.
#[test]
#[serial]
fn test_device_selection_across_mode_switches() {
    let audio = AudioManager::instance();
    audio.reset();

    // Skip if Platform mode is not available
    if audio.set_mode(AudioMode::Platform).is_err() {
        log::info!("Skipping device selection mode switch test (no audio hardware)");
        return;
    }

    let recording_count = audio.recording_devices();
    let playout_count = audio.playout_devices();

    // Select devices if available
    if recording_count > 0 {
        audio.set_recording_device(0).unwrap();
    }
    if playout_count > 0 {
        audio.set_playout_device(0).unwrap();
    }

    // Switch to Synthetic - device selection should be cleared
    audio.set_mode(AudioMode::Synthetic).unwrap();
    assert_eq!(audio.recording_devices(), 0);
    assert_eq!(audio.playout_devices(), 0);

    // Switch back to Platform - should need to re-select devices
    audio.set_mode(AudioMode::Platform).unwrap();
    assert_eq!(audio.recording_devices(), recording_count);
    assert_eq!(audio.playout_devices(), playout_count);

    // Can select devices again
    if recording_count > 0 {
        audio.set_recording_device(0).unwrap();
    }
    if playout_count > 0 {
        audio.set_playout_device(0).unwrap();
    }

    audio.reset();
}

/// Test reset() switches back to Synthetic mode.
#[test]
#[serial]
fn test_reset() {
    let audio = AudioManager::instance();

    // Try to set Platform mode first
    if audio.set_mode(AudioMode::Platform).is_ok() {
        assert_eq!(audio.current_mode(), AdmDelegateType::Platform);
    }

    // Reset should switch to Synthetic
    audio.reset();
    assert_eq!(audio.current_mode(), AdmDelegateType::Synthetic);
}

/// Test device enumeration returns 0 in Synthetic mode.
#[test]
#[serial]
fn test_device_enumeration_synthetic_mode() {
    let audio = AudioManager::instance();

    // Ensure we're in Synthetic mode
    audio.reset();

    // In Synthetic mode, device counts should be 0
    assert_eq!(audio.recording_devices(), 0);
    assert_eq!(audio.playout_devices(), 0);
}

/// Test device enumeration in Platform mode.
#[test]
#[serial]
fn test_device_enumeration_platform_mode() {
    let audio = AudioManager::instance();

    // Try to enable Platform mode
    if audio.set_mode(AudioMode::Platform).is_err() {
        log::info!("Skipping Platform mode device enumeration test (no audio hardware)");
        return;
    }

    // In Platform mode, we should have device counts >= 0
    let recording_count = audio.recording_devices();
    let playout_count = audio.playout_devices();

    log::info!(
        "Platform mode: {} recording devices, {} playout devices",
        recording_count,
        playout_count
    );

    // Device counts should be non-negative
    assert!(recording_count >= 0);
    assert!(playout_count >= 0);

    // If we have devices, test device name retrieval
    if recording_count > 0 {
        let name = audio.recording_device_name(0);
        assert!(!name.is_empty(), "Recording device name should not be empty");
        log::info!("First recording device: {}", name);
    }

    if playout_count > 0 {
        let name = audio.playout_device_name(0);
        assert!(!name.is_empty(), "Playout device name should not be empty");
        log::info!("First playout device: {}", name);
    }

    // Clean up
    audio.reset();
}

/// Test invalid device index returns error.
#[test]
#[serial]
fn test_invalid_device_index() {
    let audio = AudioManager::instance();

    // In Synthetic mode, any index should be invalid (0 devices)
    audio.reset();

    let result = audio.set_recording_device(0);
    assert!(matches!(result, Err(AudioError::InvalidDeviceIndex)));

    let result = audio.set_playout_device(0);
    assert!(matches!(result, Err(AudioError::InvalidDeviceIndex)));

    // In Platform mode, out-of-range index should be invalid
    if audio.set_mode(AudioMode::Platform).is_ok() {
        let recording_count = audio.recording_devices() as u16;
        let playout_count = audio.playout_devices() as u16;

        // Index equal to count should be invalid
        if recording_count > 0 {
            let result = audio.set_recording_device(recording_count);
            assert!(matches!(result, Err(AudioError::InvalidDeviceIndex)));
        }

        if playout_count > 0 {
            let result = audio.set_playout_device(playout_count);
            assert!(matches!(result, Err(AudioError::InvalidDeviceIndex)));
        }

        // Very large index should be invalid
        let result = audio.set_recording_device(u16::MAX);
        assert!(matches!(result, Err(AudioError::InvalidDeviceIndex)));

        audio.reset();
    }
}

/// Test device selection in Platform mode.
#[test]
#[serial]
fn test_device_selection() {
    let audio = AudioManager::instance();

    // Try to enable Platform mode
    if audio.set_mode(AudioMode::Platform).is_err() {
        log::info!("Skipping device selection test (no audio hardware)");
        return;
    }

    let recording_count = audio.recording_devices() as u16;
    let playout_count = audio.playout_devices() as u16;

    // Test selecting recording device
    if recording_count > 0 {
        let result = audio.set_recording_device(0);
        assert!(result.is_ok(), "Should be able to select recording device 0");

        // Select last device if multiple
        if recording_count > 1 {
            let result = audio.set_recording_device(recording_count - 1);
            assert!(result.is_ok(), "Should be able to select last recording device");
        }
    }

    // Test selecting playout device
    if playout_count > 0 {
        let result = audio.set_playout_device(0);
        assert!(result.is_ok(), "Should be able to select playout device 0");

        // Select last device if multiple
        if playout_count > 1 {
            let result = audio.set_playout_device(playout_count - 1);
            assert!(result.is_ok(), "Should be able to select last playout device");
        }
    }

    // Clean up
    audio.reset();
}

/// Test has_active_adm() reflects mode state.
#[test]
#[serial]
fn test_has_active_adm() {
    let audio = AudioManager::instance();

    // In Synthetic mode
    audio.reset();
    // has_active_adm may return false in synthetic mode depending on implementation
    let synthetic_has_adm = audio.has_active_adm();
    log::info!("Synthetic mode has_active_adm: {}", synthetic_has_adm);

    // In Platform mode
    if audio.set_mode(AudioMode::Platform).is_ok() {
        // Platform mode should have active ADM
        assert!(
            audio.has_active_adm(),
            "Platform mode should have active ADM"
        );

        audio.reset();
    }
}

/// Test AudioMode Display implementation.
#[test]
fn test_audio_mode_display() {
    assert_eq!(format!("{}", AudioMode::Synthetic), "Synthetic");
    assert_eq!(format!("{}", AudioMode::Platform), "Platform");
}

/// Test AudioError Display implementation.
#[test]
fn test_audio_error_display() {
    let err = AudioError::PlatformAdmInitFailed;
    assert!(format!("{}", err).contains("platform audio"));

    let err = AudioError::InvalidDeviceIndex;
    assert!(format!("{}", err).contains("Invalid device index"));

    let err = AudioError::OperationFailed("test error".to_string());
    assert!(format!("{}", err).contains("test error"));
}

/// Test AudioMode Default implementation.
#[test]
fn test_audio_mode_default() {
    let mode: AudioMode = Default::default();
    assert_eq!(mode, AudioMode::Synthetic);
}

/// Test AudioMode equality.
#[test]
fn test_audio_mode_equality() {
    assert_eq!(AudioMode::Synthetic, AudioMode::Synthetic);
    assert_eq!(AudioMode::Platform, AudioMode::Platform);
    assert_ne!(AudioMode::Synthetic, AudioMode::Platform);
}

/// Test AudioMode Clone and Copy.
#[test]
fn test_audio_mode_clone_copy() {
    let mode = AudioMode::Platform;
    let cloned = mode.clone();
    let copied = mode;

    assert_eq!(mode, cloned);
    assert_eq!(mode, copied);
}

// ============================================================================
// Integration Tests - Requires LiveKit server (__lk-e2e-test feature)
// ============================================================================

#[cfg(feature = "__lk-e2e-test")]
use {
    anyhow::{anyhow, Result},
    common::test_rooms,
    livekit::{
        options::TrackPublishOptions,
        prelude::*,
        webrtc::audio_source::RtcAudioSource,
    },
    std::time::Duration,
    tokio::time::timeout,
};

/// Integration test: Connect to room with Platform ADM and publish Device audio track.
/// Skips track publishing if no microphone is available.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_adm_room_connection() -> Result<()> {
    let audio = AudioManager::instance();

    // Enable Platform ADM before connecting
    audio.set_mode(AudioMode::Platform)?;

    // Verify Platform mode is active
    assert_eq!(audio.current_mode(), AdmDelegateType::Platform);

    // Log available devices
    let recording_count = audio.recording_devices();
    let playout_count = audio.playout_devices();
    log::info!(
        "Platform ADM: {} recording devices, {} playout devices",
        recording_count,
        playout_count
    );

    // Select default devices if available
    if recording_count > 0 {
        audio.set_recording_device(0)?;
    }
    if playout_count > 0 {
        audio.set_playout_device(0)?;
    }

    // Connect to room
    let mut rooms = test_rooms(1).await?;
    let (room, _events) = rooms.pop().unwrap();

    assert_eq!(room.connection_state(), ConnectionState::Connected);

    // Only publish audio track if we have a microphone
    if recording_count > 0 {
        // Create audio track using Device source
        let track = LocalAudioTrack::create_audio_track("microphone", RtcAudioSource::Device);

        // Publish the track
        room.local_participant()
            .publish_track(
                LocalTrack::Audio(track),
                TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..Default::default()
                },
            )
            .await?;

        log::info!("Published audio track using Platform ADM");

        // Verify track is published
        let publications = room.local_participant().track_publications();
        assert!(
            publications.values().any(|p| p.source() == TrackSource::Microphone),
            "Microphone track should be published"
        );
    } else {
        log::info!("Skipping track publish - no microphone available");
    }

    // Disconnect and reset
    room.close().await?;
    audio.reset();

    // Verify we're back to Synthetic mode
    assert_eq!(audio.current_mode(), AdmDelegateType::Synthetic);

    Ok(())
}

/// Integration test: Two participants with Platform ADM audio.
/// Skips if no microphone is available.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_adm_two_participants() -> Result<()> {
    let audio = AudioManager::instance();

    // Enable Platform ADM
    audio.set_mode(AudioMode::Platform)?;

    let recording_count = audio.recording_devices();
    let playout_count = audio.playout_devices();

    log::info!(
        "Two participants test: {} recording devices, {} playout devices",
        recording_count,
        playout_count
    );

    // This test requires a microphone to publish audio
    if recording_count == 0 {
        log::info!("Skipping two participants test - no microphone available");
        audio.reset();
        return Ok(());
    }

    audio.set_recording_device(0)?;
    if playout_count > 0 {
        audio.set_playout_device(0)?;
    }

    // Connect two participants
    let mut rooms = test_rooms(2).await?;
    let (pub_room, _) = rooms.pop().unwrap();
    let (sub_room, mut sub_events) = rooms.pop().unwrap();

    // Publisher creates and publishes audio track
    let track = LocalAudioTrack::create_audio_track("microphone", RtcAudioSource::Device);
    pub_room
        .local_participant()
        .publish_track(
            LocalTrack::Audio(track),
            TrackPublishOptions {
                source: TrackSource::Microphone,
                ..Default::default()
            },
        )
        .await?;

    log::info!("Publisher published audio track");

    // Subscriber should receive the track
    let wait_for_track = async {
        while let Some(event) = sub_events.recv().await {
            if let RoomEvent::TrackSubscribed { track, publication, participant } = event {
                log::info!(
                    "Subscriber received track from {} ({:?})",
                    participant.identity(),
                    publication.source()
                );
                assert_eq!(publication.source(), TrackSource::Microphone);
                return Ok(track);
            }
        }
        Err(anyhow!("Never received track subscription"))
    };

    timeout(Duration::from_secs(10), wait_for_track).await??;

    // Clean up
    pub_room.close().await?;
    sub_room.close().await?;
    audio.reset();

    Ok(())
}

/// Integration test: Verify teardown order (disconnect then reset).
/// Tests the proper cleanup sequence even without audio devices.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_adm_teardown_order() -> Result<()> {
    let audio = AudioManager::instance();

    // Enable Platform ADM
    audio.set_mode(AudioMode::Platform)?;

    let recording_count = audio.recording_devices();
    let playout_count = audio.playout_devices();

    log::info!(
        "Teardown order test: {} recording devices, {} playout devices",
        recording_count,
        playout_count
    );

    // Connect to room
    let mut rooms = test_rooms(1).await?;
    let (room, _events) = rooms.pop().unwrap();

    // Only publish track if we have a microphone
    if recording_count > 0 {
        let track = LocalAudioTrack::create_audio_track("microphone", RtcAudioSource::Device);
        room.local_participant()
            .publish_track(
                LocalTrack::Audio(track),
                TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..Default::default()
                },
            )
            .await?;
        log::info!("Published audio track");
    } else {
        log::info!("Skipping track publish - no microphone available");
    }

    // Correct teardown order:
    // 1. Disconnect first
    room.close().await?;
    log::info!("Room disconnected");

    // 2. Then reset audio (important for iOS VPIO release)
    audio.reset();
    log::info!("Audio reset");

    // Verify clean state
    assert_eq!(audio.current_mode(), AdmDelegateType::Synthetic);
    assert_eq!(audio.recording_devices(), 0);
    assert_eq!(audio.playout_devices(), 0);

    Ok(())
}

/// Integration test: Device hot-switching during session.
/// This test requires at least 2 recording OR 2 playout devices.
///
/// Uses `switch_recording_device()` and `switch_playout_device()` which
/// properly handle the stop/change/restart sequence for hot-swapping devices
/// while audio is active.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
#[serial]
async fn test_platform_adm_device_switching() -> Result<()> {
    let audio = AudioManager::instance();

    // Enable Platform ADM
    audio.set_mode(AudioMode::Platform)?;

    let recording_count = audio.recording_devices() as u16;
    let playout_count = audio.playout_devices() as u16;

    log::info!(
        "Device switching test: {} recording devices, {} playout devices",
        recording_count,
        playout_count
    );

    // Need at least 2 devices of ONE type to test switching
    let can_switch_recording = recording_count >= 2;
    let can_switch_playout = playout_count >= 2;

    if !can_switch_recording && !can_switch_playout {
        log::info!("Skipping device switching test (need at least 2 devices of one type)");
        audio.reset();
        return Ok(());
    }

    // Connect to room
    let mut rooms = test_rooms(1).await?;
    let (room, _events) = rooms.pop().unwrap();

    // Only publish track if we have a microphone
    if recording_count > 0 {
        let track = LocalAudioTrack::create_audio_track("microphone", RtcAudioSource::Device);
        room.local_participant()
            .publish_track(
                LocalTrack::Audio(track),
                TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..Default::default()
                },
            )
            .await?;
        log::info!("Published audio track");
    } else {
        log::info!("Skipping track publish - no microphone available");
    }

    // Use switch_recording_device / switch_playout_device which properly
    // handles the stop/change/restart sequence for hot-swapping devices

    // Switch recording devices while connected (if we have 2+)
    if can_switch_recording {
        log::info!("Switching recording device from 0 to 1 using switch_recording_device");
        audio.switch_recording_device(1)?;
        log::info!("Recording device switched to 1");

        // Small delay to let switch take effect
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Switch back
        log::info!("Switching recording device back to 0");
        audio.switch_recording_device(0)?;
        log::info!("Recording device switched to 0");
    }

    // Switch playout devices while connected (if we have 2+)
    if can_switch_playout {
        log::info!("Switching playout device from 0 to 1 using switch_playout_device");
        audio.switch_playout_device(1)?;
        log::info!("Playout device switched to 1");

        tokio::time::sleep(Duration::from_millis(100)).await;

        log::info!("Switching playout device back to 0");
        audio.switch_playout_device(0)?;
        log::info!("Playout device switched to 0");
    }

    // Clean up
    room.close().await?;
    audio.reset();

    Ok(())
}
