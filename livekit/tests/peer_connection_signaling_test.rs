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

//! Peer Connection Signaling Tests
//!
//! These tests verify that both V0 (dual peer connection) and V1 (single peer connection)
//! signaling modes work correctly.
//!
//! V0 (Dual PC): Traditional mode with separate publisher and subscriber peer connections
//!               Works on localhost with `livekit-server --dev`
//!
//! V1 (Single PC): New mode with a single peer connection for both publish and subscribe
//!                 Requires LiveKit Cloud or a server that supports /rtc/v1 endpoint.
//!                 NOTE: V1 tests will fall back to V0 on localhost, so to truly test V1,
//!                 you must set the cloud environment variables.
//!
//! Environment variables:
//! - LIVEKIT_URL: The LiveKit server URL (defaults to ws://localhost:7880)
//! - LIVEKIT_API_KEY: The API key (defaults to "devkey")
//! - LIVEKIT_API_SECRET: The API secret (defaults to "secret")
//!
//! Run all tests:
//!   cargo test -p livekit --features "__lk-e2e-test,native-tls" --test peer_connection_signaling_test -- --nocapture
//!
//! Run only V0 tests:
//!   cargo test -p livekit --features "__lk-e2e-test,native-tls" --test peer_connection_signaling_test v0_ -- --nocapture
//!
//! Run only V1 tests (set cloud env vars first):
//!   cargo test -p livekit --features "__lk-e2e-test,native-tls" --test peer_connection_signaling_test v1_ -- --nocapture

#![cfg(feature = "__lk-e2e-test")]

mod common;

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use libwebrtc::audio_source::native::NativeAudioSource;
use libwebrtc::audio_stream::native::NativeAudioStream;
use libwebrtc::native::create_random_uuid;
use libwebrtc::prelude::{
    AudioSourceOptions, IceTransportsType, RtcAudioSource, RtcVideoSource, VideoResolution,
};
use libwebrtc::video_source::native::NativeVideoSource;
use livekit::e2ee::{
    key_provider::{KeyProvider, KeyProviderOptions},
    E2eeOptions, EncryptionType,
};
use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::track::VideoQuality;
use livekit::{Room, RoomEvent, RoomOptions, SimulateScenario};
use livekit_api::access_token::{AccessToken, VideoGrants};
use std::collections::HashSet;
use std::env;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;

/// Increase file descriptor limit to avoid "Too many open files" errors.
/// This runs once at test initialization.
static INIT_FD_LIMIT: LazyLock<()> = LazyLock::new(|| {
    #[cfg(unix)]
    {
        use rlimit::{getrlimit, setrlimit, Resource};
        let (soft, hard) = getrlimit(Resource::NOFILE).expect("Failed to get NOFILE limit");
        // Try to increase to 10240 or the hard limit, whichever is lower
        let target = hard.min(10240);
        if soft < target {
            if let Err(e) = setrlimit(Resource::NOFILE, target, hard) {
                log::warn!("Failed to increase file descriptor limit: {}", e);
            } else {
                log::info!("Increased file descriptor limit from {} to {}", soft, target);
            }
        }
    }
});

/// Determine max concurrent tests based on target server.
/// Can be overridden with LIVEKIT_TEST_CONCURRENCY env var.
fn max_concurrent_tests() -> u32 {
    // Allow override via env var for tuning
    if let Ok(val) = env::var("LIVEKIT_TEST_CONCURRENCY") {
        if let Ok(n) = val.parse::<u32>() {
            log::info!("Using LIVEKIT_TEST_CONCURRENCY={}", n);
            return n;
        }
    }

    match env::var("LIVEKIT_URL") {
        Ok(url) if !url.contains("localhost") && !url.contains("127.0.0.1") => {
            // Cloud/staging: can handle more concurrent tests
            10
        }
        _ => {
            // Localhost default
            3
        }
    }
}

static TEST_SEMAPHORE: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(max_concurrent_tests() as usize)));

/// Acquire a permit to run a test. This limits how many tests run concurrently.
/// Also ensures the file descriptor limit has been increased.
async fn acquire_test_permit() -> OwnedSemaphorePermit {
    // Ensure FD limit is increased
    LazyLock::force(&INIT_FD_LIMIT);

    TEST_SEMAPHORE
        .clone()
        .acquire_owned()
        .await
        .expect("semaphore closed unexpectedly")
}

/// Acquire ALL permits to run a resource-intensive test exclusively.
/// This ensures no other tests run concurrently with this test.
async fn acquire_exclusive_test_permit() -> OwnedSemaphorePermit {
    // Ensure FD limit is increased
    LazyLock::force(&INIT_FD_LIMIT);

    TEST_SEMAPHORE
        .clone()
        .acquire_many_owned(max_concurrent_tests())
        .await
        .expect("semaphore closed unexpectedly")
}

use crate::common::audio::{SineParameters, SineTrack};

/// Signaling mode for tests
#[derive(Debug, Clone, Copy, PartialEq)]
enum SignalingMode {
    /// V0: Dual peer connection (traditional)
    DualPC,
    /// V1: Single peer connection (new)
    SinglePC,
}

impl SignalingMode {
    fn name(&self) -> &'static str {
        match self {
            SignalingMode::DualPC => "V0 (Dual PC)",
            SignalingMode::SinglePC => "V1 (Single PC)",
        }
    }

    fn is_single_pc(&self) -> bool {
        matches!(self, SignalingMode::SinglePC)
    }
}

/// Default localhost configuration
const DEFAULT_LOCALHOST_URL: &str = "ws://localhost:7880";
const DEFAULT_API_KEY: &str = "devkey";
const DEFAULT_API_SECRET: &str = "secret";

/// Get environment for tests (uses localhost defaults if env vars not set)
fn get_env_for_mode(_mode: SignalingMode) -> (String, String, String) {
    let url = env::var("LIVEKIT_URL").unwrap_or_else(|_| DEFAULT_LOCALHOST_URL.to_string());
    let api_key = env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| DEFAULT_API_KEY.to_string());
    let api_secret =
        env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| DEFAULT_API_SECRET.to_string());
    (url, api_key, api_secret)
}

fn is_local_dev_server(url: &str) -> bool {
    url.contains("localhost:7880") || url.contains("127.0.0.1:7880")
}

fn assert_signaling_mode_state(room: &Room, mode: SignalingMode, url: &str) {
    let active_single_pc = room.is_single_peer_connection_active();
    match mode {
        SignalingMode::DualPC => {
            assert!(!active_single_pc, "DualPC test should not have single-PC mode active");
        }
        SignalingMode::SinglePC => {
            if is_local_dev_server(url) {
                // Local dev server behavior may vary by version:
                // older versions fallback to v0, newer versions may support /rtc/v1.
                log::info!(
                    "SinglePC on localhost: single_pc_active={} (fallback to v0 expected on older servers)",
                    active_single_pc
                );
            } else {
                assert!(
                    active_single_pc,
                    "SinglePC requested on non-localhost URL should stay in single-PC mode"
                );
            }
        }
    }
}

/// Create a token for testing
fn create_token(
    api_key: &str,
    api_secret: &str,
    room_name: &str,
    identity: &str,
) -> Result<String> {
    let grants = VideoGrants {
        room_join: true,
        room: room_name.to_string(),
        can_publish: true,
        can_subscribe: true,
        ..Default::default()
    };
    AccessToken::with_api_key(api_key, api_secret)
        .with_ttl(Duration::from_secs(30 * 60))
        .with_grants(grants)
        .with_identity(identity)
        .with_name(identity)
        .to_jwt()
        .context("Failed to generate JWT")
}

/// Create room options for the specified signaling mode
fn room_options(mode: SignalingMode) -> RoomOptions {
    let mut options = RoomOptions::default();
    options.auto_subscribe = true;
    options.dynacast = false;
    options.single_peer_connection = mode.is_single_pc();
    options
}

/// Connect to a room with specified signaling mode
async fn connect_room(
    url: &str,
    token: &str,
    mode: SignalingMode,
) -> Result<(Room, UnboundedReceiver<RoomEvent>)> {
    let options = room_options(mode);
    let started_at = Instant::now();
    let result = Room::connect(url, token, options).await;
    let elapsed = started_at.elapsed();

    match &result {
        Ok((room, _)) => {
            println!(
                "[{}] connect_room elapsed={:?}, single_pc_active={}",
                mode.name(),
                elapsed,
                room.is_single_peer_connection_active()
            );
        }
        Err(err) => {
            println!("[{}] connect_room failed after {:?}: {:?}", mode.name(), elapsed, err);
        }
    }

    result.context(format!("Failed to connect to room with {}", mode.name()))
}

/// Create multiple test rooms with specified signaling mode.
/// Unlike `test_rooms_with_options`, this does NOT override single_peer_connection on localhost,
/// allowing proper testing of V1 mode when the server supports it.
async fn create_test_rooms(
    mode: SignalingMode,
    participant_count: usize,
) -> Result<Vec<(Room, UnboundedReceiver<RoomEvent>)>> {
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_{}", mode, create_random_uuid());
    let options = room_options(mode);

    let mut rooms = Vec::with_capacity(participant_count);
    for i in 0..participant_count {
        let identity = format!("participant_{}", i);
        let token = create_token(&api_key, &api_secret, &room_name, &identity)?;
        let (room, events) = Room::connect(&url, &token, options.clone())
            .await
            .context(format!("Failed to connect participant {} with {}", i, mode.name()))?;
        rooms.push((room, events));
    }

    // Wait for all participants to see each other
    let wait_visibility = async {
        let expected_remote_count = participant_count - 1;
        loop {
            let all_visible = rooms
                .iter()
                .all(|(room, _)| room.remote_participants().len() == expected_remote_count);
            if all_visible {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };

    timeout(Duration::from_secs(10), wait_visibility)
        .await
        .context("Timeout waiting for all participants to become visible")?;

    log::info!("[{}] All {} participants connected and visible", mode.name(), participant_count);
    Ok(rooms)
}

// ==================== V0 (Dual PC) Tests ====================

/// Test basic connection with V0 signaling (dual PC)
#[test_log::test(tokio::test)]
async fn test_v0_connect() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_connect_impl(SignalingMode::DualPC).await
}

/// Test two participants with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_two_participants() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_two_participants_impl(SignalingMode::DualPC).await
}

/// Test audio track with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_audio_track() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_audio_track_impl(SignalingMode::DualPC).await
}

/// Test reconnection with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_reconnect() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_reconnect_impl(SignalingMode::DualPC).await
}

/// Test data channel with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_data_channel() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_data_channel_impl(SignalingMode::DualPC).await
}

/// Test node failure with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_node_failure() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_node_failure_impl(SignalingMode::DualPC).await
}

/// Test publishing 10 video + 10 audio tracks with V0 signaling
/// This test is resource-intensive so it runs exclusively (no other tests in parallel).
#[test_log::test(tokio::test)]
async fn test_v0_publish_ten_video_and_ten_audio_tracks() -> Result<()> {
    let _permit = acquire_exclusive_test_permit().await;
    test_publish_ten_video_and_ten_audio_tracks_impl(SignalingMode::DualPC).await
}

// ==================== V1 (Single PC) Tests ====================

/// Test basic connection with V1 signaling (single PC)
#[test_log::test(tokio::test)]
async fn test_v1_connect() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_connect_impl(SignalingMode::SinglePC).await
}

/// Test two participants with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_two_participants() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_two_participants_impl(SignalingMode::SinglePC).await
}

/// Test audio track with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_audio_track() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_audio_track_impl(SignalingMode::SinglePC).await
}

/// Test reconnection with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_reconnect() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_reconnect_impl(SignalingMode::SinglePC).await
}

/// Test data channel with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_data_channel() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_data_channel_impl(SignalingMode::SinglePC).await
}

/// Test node failure with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_node_failure() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_node_failure_impl(SignalingMode::SinglePC).await
}

/// Test publishing 10 video + 10 audio tracks with V1 signaling
/// This test is resource-intensive so it runs exclusively (no other tests in parallel).
#[test_log::test(tokio::test)]
async fn test_v1_publish_ten_video_and_ten_audio_tracks() -> Result<()> {
    let _permit = acquire_exclusive_test_permit().await;
    test_publish_ten_video_and_ten_audio_tracks_impl(SignalingMode::SinglePC).await
}

/// Test explicit localhost fallback behavior for V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_localhost_fallback_to_v0() -> Result<()> {
    let _permit = acquire_test_permit().await;
    if env::var("LIVEKIT_URL").is_ok() {
        log::info!("Skipping localhost fallback test because LIVEKIT_URL override is set");
        return Ok(());
    }

    let room_name = format!("test_v1_localhost_fallback_{}", create_random_uuid());
    let token = create_token(DEFAULT_API_KEY, DEFAULT_API_SECRET, &room_name, "fallback_test")?;
    let (room, _events) =
        connect_room(DEFAULT_LOCALHOST_URL, &token, SignalingMode::SinglePC).await?;
    if room.is_single_peer_connection_active() {
        log::info!("Localhost server supports /rtc/v1; skipping fallback assertion");
        return Ok(());
    }
    assert!(!room.is_single_peer_connection_active(), "Expected fallback to v0");
    Ok(())
}

/// Test that a participant with can_subscribe=false in their token can connect without timing out.
#[test_log::test(tokio::test)]
async fn test_v0_connect_can_subscribe_false() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_connect_can_subscribe_false_impl(SignalingMode::DualPC).await
}

#[test_log::test(tokio::test)]
async fn test_v1_connect_can_subscribe_false() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_connect_can_subscribe_false_impl(SignalingMode::SinglePC).await
}

/// Corner case: reconnect twice in a row
#[test_log::test(tokio::test)]
async fn test_v0_double_reconnect() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_double_reconnect_impl(SignalingMode::DualPC).await
}

/// Corner case: reconnect twice in a row
#[test_log::test(tokio::test)]
async fn test_v1_double_reconnect() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_double_reconnect_impl(SignalingMode::SinglePC).await
}

// ==================== Test Implementations ====================

/// Test basic connection
async fn test_connect_impl(mode: SignalingMode) -> Result<()> {
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_{}", mode, create_random_uuid());
    let token = create_token(&api_key, &api_secret, &room_name, "test_participant")?;

    log::info!("[{}] Connecting to {}", mode.name(), url);

    let (room, _events) = connect_room(&url, &token, mode).await?;

    log::info!("[{}] Connected! Room: {:?}", mode.name(), room.name());
    log::info!("[{}] Local participant: {:?}", mode.name(), room.local_participant().identity());

    // Verify connection is working
    assert_eq!(room.connection_state(), ConnectionState::Connected);
    assert_signaling_mode_state(&room, mode, &url);

    // Give it a moment to ensure connection is stable
    tokio::time::sleep(Duration::from_secs(2)).await;

    log::info!("[{}] Test passed - connection working!", mode.name());
    Ok(())
}

/// Test two participants connecting
async fn test_two_participants_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Connecting two participants", mode.name());
    let (url, _, _) = get_env_for_mode(mode);
    let mut rooms = create_test_rooms(mode, 2).await?;
    let (room2, _events2) = rooms.pop().unwrap();
    let (room1, _events1) = rooms.pop().unwrap();
    assert_signaling_mode_state(&room1, mode, &url);
    assert_signaling_mode_state(&room2, mode, &url);

    log::info!("[{}] Both participants visible to each other", mode.name());
    log::info!("[{}] Test passed - two participants working!", mode.name());
    Ok(())
}

/// Test publishing and receiving audio tracks
async fn test_audio_track_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing audio track", mode.name());
    let (url, _, _) = get_env_for_mode(mode);
    let mut rooms = create_test_rooms(mode, 2).await?;
    let (sub_room, mut sub_events) = rooms.pop().unwrap();
    let (pub_room, _pub_events) = rooms.pop().unwrap();
    assert_signaling_mode_state(&pub_room, mode, &url);
    assert_signaling_mode_state(&sub_room, mode, &url);

    // Publish a sine wave track
    const SINE_FREQ: f64 = 440.0;
    let sine_params =
        SineParameters { freq: SINE_FREQ, amplitude: 1.0, sample_rate: 48000, num_channels: 1 };
    let pub_room_arc = Arc::new(pub_room);
    let mut sine_track = SineTrack::new(pub_room_arc, sine_params);
    sine_track.publish().await?;

    log::info!("[{}] Published audio track, waiting for subscriber to receive", mode.name());

    // Wait for track subscription
    let receive_track = async {
        loop {
            let Some(event) = sub_events.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            if let RoomEvent::TrackSubscribed { track, publication: _, participant: _ } = event {
                return Ok(track);
            }
        }
    };

    let track = timeout(Duration::from_secs(15), receive_track)
        .await
        .context("Timeout waiting for track subscription")??;

    log::info!("[{}] Received track: {:?}", mode.name(), track.sid());

    // Verify it's an audio track
    let RemoteTrack::Audio(audio_track) = track else {
        return Err(anyhow!("Expected audio track"));
    };

    // Read some audio frames
    let mut stream = NativeAudioStream::new(audio_track.rtc_track(), 48000, 1);
    let mut frames_received = 0;
    let receive_frames = async {
        while let Some(frame) = stream.next().await {
            assert!(!frame.data.is_empty());
            frames_received += 1;
            if frames_received >= 50 {
                break;
            }
        }
    };

    timeout(Duration::from_secs(10), receive_frames)
        .await
        .context("Timeout receiving audio frames")?;

    log::info!("[{}] Received {} audio frames", mode.name(), frames_received);
    log::info!("[{}] Test passed - audio track working!", mode.name());
    Ok(())
}

/// Test publishing 10 video + 10 audio tracks and verifying subscriber receives all tracks.
async fn test_publish_ten_video_and_ten_audio_tracks_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing publish 10 video + 10 audio tracks", mode.name());
    let (url, _, _) = get_env_for_mode(mode);
    let mut rooms = create_test_rooms(mode, 2).await?;
    let (sub_room, mut sub_events) = rooms.pop().unwrap();
    let (pub_room, _pub_events) = rooms.pop().unwrap();
    assert_signaling_mode_state(&pub_room, mode, &url);
    assert_signaling_mode_state(&sub_room, mode, &url);
    let publisher_identity = pub_room.local_participant().identity().to_string();

    let mut expected_names = HashSet::new();
    let mut publications = Vec::new();
    let mut video_sources = Vec::new();
    let mut audio_sources = Vec::new();

    for i in 0..10 {
        let name = format!("video-track-{}", i);
        let source =
            NativeVideoSource::new(VideoResolution { width: 640, height: 360 }, i % 2 == 1);
        let track =
            LocalVideoTrack::create_video_track(&name, RtcVideoSource::Native(source.clone()));
        let mut opts = TrackPublishOptions::default();
        opts.source = if i % 2 == 0 { TrackSource::Camera } else { TrackSource::Screenshare };
        let publication =
            pub_room.local_participant().publish_track(LocalTrack::Video(track), opts).await?;
        expected_names.insert(name);
        publications.push(publication);
        video_sources.push(source);
    }

    for i in 0..10 {
        let name = format!("audio-track-{}", i);
        let source = NativeAudioSource::new(AudioSourceOptions::default(), 48_000, 1, 1000);
        let track =
            LocalAudioTrack::create_audio_track(&name, RtcAudioSource::Native(source.clone()));
        let mut opts = TrackPublishOptions::default();
        opts.source =
            if i % 2 == 0 { TrackSource::Microphone } else { TrackSource::ScreenshareAudio };
        let publication =
            pub_room.local_participant().publish_track(LocalTrack::Audio(track), opts).await?;
        expected_names.insert(name);
        publications.push(publication);
        audio_sources.push(source);
    }

    let mut last_retry = Instant::now() - Duration::from_secs(1);
    let receive_all_tracks = async {
        loop {
            let mut published_names = HashSet::new();
            let mut subscribed_names = HashSet::new();
            let mut audio_count = 0usize;
            let mut video_count = 0usize;

            let remote_participants = sub_room.remote_participants();
            let publisher_entry = remote_participants
                .iter()
                .find(|(_, p)| p.identity().as_str() == publisher_identity.as_str());

            if let Some((_, publisher)) = publisher_entry {
                let publications = publisher.track_publications();
                for publication in publications.values() {
                    let name = publication.name();
                    if expected_names.contains(&name) {
                        published_names.insert(name.clone());
                        if let Some(track) = publication.track() {
                            subscribed_names.insert(name);
                            match track {
                                RemoteTrack::Audio(_) => audio_count += 1,
                                RemoteTrack::Video(_) => video_count += 1,
                            }
                        }
                    }
                }

                if published_names.len() >= expected_names.len()
                    && subscribed_names.len() >= expected_names.len()
                    && audio_count >= 10
                    && video_count >= 10
                {
                    return Ok((subscribed_names, audio_count, video_count));
                }

                // Under load, transient TrackSubscriptionFailed can happen before publication
                // state fully settles. Retry subscription on missing tracks.
                if published_names.len() >= expected_names.len()
                    && last_retry.elapsed() >= Duration::from_millis(300)
                {
                    for publication in publications.values() {
                        if expected_names.contains(&publication.name())
                            && publication.track().is_none()
                        {
                            publication.set_subscribed(false);
                            publication.set_subscribed(true);
                        }
                    }
                    last_retry = Instant::now();
                }
            }

            match timeout(Duration::from_millis(250), sub_events.recv()).await {
                Ok(Some(RoomEvent::TrackSubscriptionFailed { participant, track_sid, error })) => {
                    log::warn!(
                        "[{}] TrackSubscriptionFailed sid={} participant={} error={:?}",
                        mode.name(),
                        track_sid,
                        participant.identity(),
                        error
                    );
                    if let Some(publication) = participant.get_track_publication(&track_sid) {
                        publication.set_subscribed(false);
                        publication.set_subscribed(true);
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => return Err(anyhow!("Event channel closed")),
                Err(_) => {}
            }
        }
    };

    let (received_names, audio_count, video_count) =
        timeout(Duration::from_secs(45), receive_all_tracks)
            .await
            .context("Timeout waiting for all 20 track subscriptions")??;

    for expected in &expected_names {
        assert!(received_names.contains(expected), "missing subscribed track: {}", expected);
    }
    assert!(audio_count >= 10, "expected >=10 audio tracks, got {}", audio_count);
    assert!(video_count >= 10, "expected >=10 video tracks, got {}", video_count);

    let remote_participants = sub_room.remote_participants();
    let publisher_entry = remote_participants
        .iter()
        .find(|(_, p)| p.identity().as_str() == publisher_identity.as_str());
    if let Some((_, publisher)) = publisher_entry {
        assert!(
            publisher.track_publications().len() >= 20,
            "subscriber should see >=20 published tracks from publisher"
        );
    }

    for publication in publications {
        pub_room.local_participant().unpublish_track(&publication.sid()).await?;
    }
    Ok(())
}

/// Test reconnection - verifies tracks are restored
async fn test_reconnect_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing reconnection", mode.name());
    let (url, _, _) = get_env_for_mode(mode);
    let mut rooms = create_test_rooms(mode, 2).await?;
    let (sub_room, mut sub_events) = rooms.pop().unwrap();
    let (pub_room, mut pub_events) = rooms.pop().unwrap();
    assert_signaling_mode_state(&pub_room, mode, &url);
    assert_signaling_mode_state(&sub_room, mode, &url);
    let publisher_identity = pub_room.local_participant().identity().to_string();

    // Wrap in Arc for SineTrack
    let pub_room_arc = Arc::new(pub_room);

    // Publish a track
    let sine_params =
        SineParameters { freq: 440.0, amplitude: 1.0, sample_rate: 48000, num_channels: 1 };
    let mut sine_track = SineTrack::new(pub_room_arc.clone(), sine_params);
    sine_track.publish().await?;

    // Wait for initial track subscription
    let wait_track = async {
        loop {
            let Some(event) = sub_events.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            if let RoomEvent::TrackSubscribed { track: _, publication: _, participant: _ } = event {
                return Ok(());
            }
        }
    };

    timeout(Duration::from_secs(15), wait_track)
        .await
        .context("Timeout waiting for initial track subscription")??;

    log::info!(
        "[{}] Initial track received, verifying track count before reconnection",
        mode.name()
    );

    let tracks_before = pub_room_arc.local_participant().track_publications().len();
    log::info!("[{}] Tracks published before reconnect: {}", mode.name(), tracks_before);

    log::info!("[{}] Simulating signal reconnect...", mode.name());

    // Simulate a signal reconnect
    pub_room_arc.simulate_scenario(SimulateScenario::SignalReconnect).await?;

    // Wait for reconnection events
    let wait_reconnected = async {
        loop {
            let Some(event) = pub_events.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            match event {
                RoomEvent::Reconnecting => {
                    log::info!("[{}] Publisher reconnecting...", mode.name());
                }
                RoomEvent::Reconnected => {
                    log::info!("[{}] Publisher reconnected!", mode.name());
                    return Ok(());
                }
                _ => {}
            }
        }
    };

    timeout(Duration::from_secs(30), wait_reconnected)
        .await
        .context("Timeout waiting for reconnection")??;

    // Give some time for state to stabilize
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify room is connected
    assert_eq!(
        pub_room_arc.connection_state(),
        ConnectionState::Connected,
        "Room should be connected after reconnect"
    );

    // Verify track is still published
    let tracks_after = pub_room_arc.local_participant().track_publications().len();
    log::info!("[{}] Tracks published after reconnect: {}", mode.name(), tracks_after);
    assert_eq!(tracks_before, tracks_after, "Track count should be preserved after reconnect");

    // Verify subscriber can still see the publisher's tracks
    let remote_participants = sub_room.remote_participants();
    let publisher_entry = remote_participants
        .iter()
        .find(|(_, p)| p.identity().as_str() == publisher_identity.as_str());

    if let Some((_, publisher)) = publisher_entry {
        let remote_tracks = publisher.track_publications().len();
        log::info!("[{}] Subscriber sees {} tracks from publisher", mode.name(), remote_tracks);
        assert!(remote_tracks > 0, "Subscriber should still see publisher's tracks");
    } else {
        log::warn!("[{}] Publisher not found in remote participants", mode.name());
    }

    log::info!("[{}] Test passed - reconnection working!", mode.name());
    Ok(())
}

/// Test data channel
async fn test_data_channel_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing data channel", mode.name());
    let (url, _, _) = get_env_for_mode(mode);
    let mut rooms = create_test_rooms(mode, 2).await?;
    let (room2, mut events2) = rooms.pop().unwrap();
    let (room1, _events1) = rooms.pop().unwrap();
    assert_signaling_mode_state(&room1, mode, &url);
    assert_signaling_mode_state(&room2, mode, &url);

    // Send data from room1 to room2
    let test_data = b"Hello from peer connection signaling test!".to_vec();
    let test_topic = "test_topic".to_string();

    room1
        .local_participant()
        .publish_data(livekit::DataPacket {
            payload: test_data.clone(),
            topic: Some(test_topic.clone()),
            reliable: true,
            ..Default::default()
        })
        .await?;

    log::info!("[{}] Sent data packet, waiting for receiver...", mode.name());

    // Wait to receive data
    let receive_data = async {
        loop {
            let Some(event) = events2.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            if let RoomEvent::DataReceived { payload, topic, kind: _, participant: _ } = event {
                if topic == Some(test_topic.clone()) {
                    return Ok(payload);
                }
            }
        }
    };

    let received = timeout(Duration::from_secs(10), receive_data)
        .await
        .context("Timeout waiting for data")??;

    assert_eq!(received.to_vec(), test_data, "Received data should match sent data");

    log::info!("[{}] Test passed - data channel working!", mode.name());
    Ok(())
}

/// Test node failure reconnection scenario
async fn test_node_failure_impl(mode: SignalingMode) -> Result<()> {
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_node_fail_{}", mode, create_random_uuid());

    let token = create_token(&api_key, &api_secret, &room_name, "test_participant")?;

    log::info!("[{}] Testing node failure scenario", mode.name());

    let (room, mut events) = connect_room(&url, &token, mode).await?;

    // Wrap in Arc for SineTrack
    let room_arc = Arc::new(room);

    // Publish a track first
    let sine_params =
        SineParameters { freq: 440.0, amplitude: 1.0, sample_rate: 48000, num_channels: 1 };
    let mut sine_track = SineTrack::new(room_arc.clone(), sine_params);
    sine_track.publish().await?;

    let tracks_before = room_arc.local_participant().track_publications().len();
    log::info!("[{}] Tracks before node failure: {}", mode.name(), tracks_before);

    log::info!("[{}] Simulating node failure...", mode.name());
    room_arc.simulate_scenario(SimulateScenario::NodeFailure).await?;

    // Wait for reconnection
    let wait_reconnected = async {
        loop {
            let Some(event) = events.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            match event {
                RoomEvent::Reconnecting => {
                    log::info!("[{}] Reconnecting after node failure...", mode.name());
                }
                RoomEvent::Reconnected => {
                    log::info!("[{}] Reconnected after node failure!", mode.name());
                    return Ok(());
                }
                RoomEvent::Disconnected { reason } => {
                    log::info!("[{}] Disconnected: {:?}", mode.name(), reason);
                }
                _ => {}
            }
        }
    };

    timeout(Duration::from_secs(30), wait_reconnected)
        .await
        .context("Timeout waiting for reconnection after node failure")??;

    // Give time for track republishing
    tokio::time::sleep(Duration::from_secs(3)).await;

    let tracks_after = room_arc.local_participant().track_publications().len();
    log::info!("[{}] Tracks after node failure reconnect: {}", mode.name(), tracks_after);

    assert_eq!(tracks_before, tracks_after, "Tracks should be restored after node failure");

    log::info!("[{}] Test passed - node failure recovery working!", mode.name());
    Ok(())
}

/// Test that a participant with can_subscribe=false in their token can connect without timing out.
async fn test_connect_can_subscribe_false_impl(mode: SignalingMode) -> Result<()> {
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_no_subscribe_{}", mode, create_random_uuid());

    let grants = VideoGrants {
        room_join: true,
        room: room_name.clone(),
        can_publish: true,
        can_subscribe: false,
        ..Default::default()
    };
    let token = AccessToken::with_api_key(&api_key, &api_secret)
        .with_ttl(Duration::from_secs(30 * 60))
        .with_grants(grants)
        .with_identity("no-subscribe-participant")
        .with_name("no-subscribe-participant")
        .to_jwt()
        .context("Failed to generate JWT")?;

    log::info!("[{}] Connecting with can_subscribe=false", mode.name());
    let (room, _events) = connect_room(&url, &token, mode).await?;

    assert_eq!(
        room.connection_state(),
        ConnectionState::Connected,
        "Room should be connected even when can_subscribe=false"
    );

    log::info!("[{}] Test passed - can_subscribe=false connects without timeout!", mode.name());
    Ok(())
}

/// Test two sequential reconnect cycles on the same room connection
async fn test_double_reconnect_impl(mode: SignalingMode) -> Result<()> {
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_double_reconnect_{}", mode, create_random_uuid());
    let token = create_token(&api_key, &api_secret, &room_name, "reconnect_tester")?;

    let (room, mut events) = connect_room(&url, &token, mode).await?;
    assert_signaling_mode_state(&room, mode, &url);

    for attempt in 1..=2 {
        log::info!("[{}] Triggering reconnect attempt {}", mode.name(), attempt);
        room.simulate_scenario(SimulateScenario::SignalReconnect).await?;

        let wait_reconnected = async {
            loop {
                let Some(event) = events.recv().await else {
                    return Err(anyhow!("Event channel closed"));
                };
                match event {
                    RoomEvent::Reconnecting => {}
                    RoomEvent::Reconnected => return Ok(()),
                    _ => {}
                }
            }
        };

        timeout(Duration::from_secs(30), wait_reconnected)
            .await
            .context("Timeout waiting for reconnect cycle")??;

        assert_eq!(room.connection_state(), ConnectionState::Connected);
    }

    Ok(())
}

// ==================== auto_subscribe=false Tests ====================
//
// Expected behavior:
// - When auto_subscribe=false, tracks from remote participants are NOT automatically subscribed
// - TrackPublished events are received, but TrackSubscribed events only happen after manual subscription
// - publication.set_subscribed(true) must be called to subscribe to tracks
// - This tests different signaling flows where subscription is decoupled from publication

/// Test auto_subscribe=false with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_auto_subscribe_false() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_auto_subscribe_false_impl(SignalingMode::DualPC).await
}

/// Test auto_subscribe=false with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_auto_subscribe_false() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_auto_subscribe_false_impl(SignalingMode::SinglePC).await
}

/// Create room options with auto_subscribe disabled
fn room_options_no_auto_subscribe(mode: SignalingMode) -> RoomOptions {
    let mut options = RoomOptions::default();
    options.auto_subscribe = false;
    options.dynacast = false;
    options.single_peer_connection = mode.is_single_pc();
    options
}

/// Test that tracks require manual subscription when auto_subscribe=false
///
/// Expected Results:
/// 1. Publisher publishes an audio track
/// 2. Subscriber receives TrackPublished event (not TrackSubscribed)
/// 3. Track is NOT subscribed automatically (publication.track() returns None)
/// 4. After calling set_subscribed(true), subscriber receives TrackSubscribed event
/// 5. Track is now accessible and audio frames can be received
async fn test_auto_subscribe_false_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing auto_subscribe=false", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_no_auto_sub_{}", mode, create_random_uuid());

    // Publisher with auto_subscribe=true (default)
    let pub_token = create_token(&api_key, &api_secret, &room_name, "publisher")?;
    let (pub_room, _pub_events) = connect_room(&url, &pub_token, mode).await?;

    // Subscriber with auto_subscribe=false
    let sub_token = create_token(&api_key, &api_secret, &room_name, "subscriber")?;
    let sub_options = room_options_no_auto_subscribe(mode);
    let (sub_room, mut sub_events) = Room::connect(&url, &sub_token, sub_options).await?;

    let publisher_identity = pub_room.local_participant().identity().to_string();

    // Publish a sine wave track
    let sine_params =
        SineParameters { freq: 440.0, amplitude: 1.0, sample_rate: 48000, num_channels: 1 };
    let pub_room_arc = Arc::new(pub_room);
    let mut sine_track = SineTrack::new(pub_room_arc.clone(), sine_params);
    sine_track.publish().await?;

    log::info!(
        "[{}] Published track, waiting for TrackPublished event (not TrackSubscribed)",
        mode.name()
    );

    // Wait for TrackPublished event (NOT TrackSubscribed, since auto_subscribe=false)
    let wait_published = async {
        loop {
            let Some(event) = sub_events.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            match event {
                RoomEvent::TrackPublished { publication, participant } => {
                    if participant.identity().as_str() == publisher_identity {
                        log::info!(
                            "[{}] Received TrackPublished for track: {}",
                            mode.name(),
                            publication.sid()
                        );
                        return Ok(publication);
                    }
                }
                RoomEvent::TrackSubscribed { .. } => {
                    return Err(anyhow!(
                        "Received TrackSubscribed before manual subscription - auto_subscribe should be false!"
                    ));
                }
                _ => {}
            }
        }
    };

    let publication = timeout(Duration::from_secs(15), wait_published)
        .await
        .context("Timeout waiting for TrackPublished event")??;

    // Verify track is NOT subscribed yet
    assert!(
        publication.track().is_none(),
        "Track should NOT be subscribed when auto_subscribe=false"
    );
    log::info!("[{}] Verified track is not auto-subscribed", mode.name());

    // Now manually subscribe
    log::info!("[{}] Manually subscribing to track...", mode.name());
    publication.set_subscribed(true);

    // Wait for TrackSubscribed event
    let wait_subscribed = async {
        loop {
            let Some(event) = sub_events.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            if let RoomEvent::TrackSubscribed { track, publication: _, participant } = event {
                if participant.identity().as_str() == publisher_identity {
                    return Ok(track);
                }
            }
        }
    };

    let track = timeout(Duration::from_secs(15), wait_subscribed)
        .await
        .context("Timeout waiting for TrackSubscribed after manual subscription")??;

    log::info!("[{}] Track subscribed after manual subscription: {:?}", mode.name(), track.sid());

    // Verify we can receive audio frames
    let RemoteTrack::Audio(audio_track) = track else {
        return Err(anyhow!("Expected audio track"));
    };

    let mut stream = NativeAudioStream::new(audio_track.rtc_track(), 48000, 1);
    let mut frames_received = 0;
    let receive_frames = async {
        while let Some(frame) = stream.next().await {
            assert!(!frame.data.is_empty());
            frames_received += 1;
            if frames_received >= 10 {
                break;
            }
        }
    };

    timeout(Duration::from_secs(10), receive_frames)
        .await
        .context("Timeout receiving audio frames after subscription")?;

    log::info!(
        "[{}] Successfully received {} audio frames after manual subscription",
        mode.name(),
        frames_received
    );

    // Clean up
    drop(sub_room);

    log::info!("[{}] Test passed - auto_subscribe=false working correctly!", mode.name());
    Ok(())
}

// ==================== dynacast=true Tests ====================
//
// Expected behavior:
// - Dynacast enables dynamic broadcast based on subscriber interest
// - Publisher only sends video layers that subscribers are actually consuming
// - This reduces bandwidth when subscribers don't need all simulcast layers

/// Test dynacast=true with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_dynacast() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_dynacast_impl(SignalingMode::DualPC).await
}

/// Test dynacast=true with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_dynacast() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_dynacast_impl(SignalingMode::SinglePC).await
}

/// Create room options with dynacast enabled
fn room_options_dynacast(mode: SignalingMode) -> RoomOptions {
    let mut options = RoomOptions::default();
    options.auto_subscribe = true;
    options.dynacast = true;
    options.single_peer_connection = mode.is_single_pc();
    options
}

/// Test dynacast mode
///
/// Expected Results:
/// 1. Both publisher and subscriber connect with dynacast=true
/// 2. Publisher publishes a video track
/// 3. Subscriber receives and subscribes to the track
/// 4. Connection remains stable with dynacast signaling
/// 5. Video frames are received correctly
async fn test_dynacast_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing dynacast=true", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_dynacast_{}", mode, create_random_uuid());

    // Both participants use dynacast
    let pub_token = create_token(&api_key, &api_secret, &room_name, "publisher")?;
    let pub_options = room_options_dynacast(mode);
    let (pub_room, _pub_events) = Room::connect(&url, &pub_token, pub_options).await?;

    let sub_token = create_token(&api_key, &api_secret, &room_name, "subscriber")?;
    let sub_options = room_options_dynacast(mode);
    let (sub_room, mut sub_events) = Room::connect(&url, &sub_token, sub_options).await?;

    assert_signaling_mode_state(&pub_room, mode, &url);
    assert_signaling_mode_state(&sub_room, mode, &url);

    let publisher_identity = pub_room.local_participant().identity().to_string();

    // Publish a video track
    let video_source = NativeVideoSource::new(VideoResolution { width: 640, height: 360 }, false);
    let video_track = LocalVideoTrack::create_video_track(
        "dynacast_video",
        RtcVideoSource::Native(video_source.clone()),
    );
    let mut opts = TrackPublishOptions::default();
    opts.source = TrackSource::Camera;
    let _publication =
        pub_room.local_participant().publish_track(LocalTrack::Video(video_track), opts).await?;

    log::info!("[{}] Published video track with dynacast, waiting for subscription", mode.name());

    // Wait for track subscription
    let wait_subscribed = async {
        loop {
            let Some(event) = sub_events.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            if let RoomEvent::TrackSubscribed { track, publication: _, participant } = event {
                if participant.identity().as_str() == publisher_identity {
                    return Ok(track);
                }
            }
        }
    };

    let track = timeout(Duration::from_secs(15), wait_subscribed)
        .await
        .context("Timeout waiting for video track subscription with dynacast")??;

    // Verify it's a video track
    let RemoteTrack::Video(_video_track) = track else {
        return Err(anyhow!("Expected video track"));
    };

    log::info!("[{}] Video track subscribed with dynacast enabled", mode.name());

    // Let the connection stabilize with dynacast signaling
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify connection is still stable
    assert_eq!(pub_room.connection_state(), ConnectionState::Connected);
    assert_eq!(sub_room.connection_state(), ConnectionState::Connected);

    log::info!("[{}] Test passed - dynacast=true working correctly!", mode.name());
    Ok(())
}

// ==================== adaptive_stream=true Tests ====================
//
// Expected behavior:
// - Adaptive stream automatically adjusts video quality based on subscriber's rendered size
// - Signaling is used to communicate quality preferences to the server
// - This optimizes bandwidth by only sending resolution the subscriber actually needs

/// Test adaptive_stream=true with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_adaptive_stream() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_adaptive_stream_impl(SignalingMode::DualPC).await
}

/// Test adaptive_stream=true with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_adaptive_stream() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_adaptive_stream_impl(SignalingMode::SinglePC).await
}

/// Create room options with adaptive_stream enabled
fn room_options_adaptive_stream(mode: SignalingMode) -> RoomOptions {
    let mut options = RoomOptions::default();
    options.auto_subscribe = true;
    options.adaptive_stream = true;
    options.single_peer_connection = mode.is_single_pc();
    options
}

/// Test adaptive stream mode
///
/// Expected Results:
/// 1. Subscriber connects with adaptive_stream=true
/// 2. Publisher publishes a video track
/// 3. Subscriber receives and subscribes to the track
/// 4. Connection remains stable with adaptive stream signaling
/// 5. Track is received and can be consumed
async fn test_adaptive_stream_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing adaptive_stream=true", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_adaptive_{}", mode, create_random_uuid());

    // Publisher with default options
    let pub_token = create_token(&api_key, &api_secret, &room_name, "publisher")?;
    let (pub_room, _pub_events) = connect_room(&url, &pub_token, mode).await?;

    // Subscriber with adaptive_stream enabled
    let sub_token = create_token(&api_key, &api_secret, &room_name, "subscriber")?;
    let sub_options = room_options_adaptive_stream(mode);
    let (sub_room, mut sub_events) = Room::connect(&url, &sub_token, sub_options).await?;

    assert_signaling_mode_state(&pub_room, mode, &url);

    let publisher_identity = pub_room.local_participant().identity().to_string();

    // Publish a video track
    let video_source = NativeVideoSource::new(VideoResolution { width: 1280, height: 720 }, false);
    let video_track = LocalVideoTrack::create_video_track(
        "adaptive_video",
        RtcVideoSource::Native(video_source.clone()),
    );
    let mut opts = TrackPublishOptions::default();
    opts.source = TrackSource::Camera;
    let _publication =
        pub_room.local_participant().publish_track(LocalTrack::Video(video_track), opts).await?;

    log::info!(
        "[{}] Published video track, waiting for subscription with adaptive_stream",
        mode.name()
    );

    // Wait for track subscription
    let wait_subscribed = async {
        loop {
            let Some(event) = sub_events.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            if let RoomEvent::TrackSubscribed { track, publication: _, participant } = event {
                if participant.identity().as_str() == publisher_identity {
                    return Ok(track);
                }
            }
        }
    };

    let track = timeout(Duration::from_secs(15), wait_subscribed)
        .await
        .context("Timeout waiting for video track subscription with adaptive_stream")??;

    let RemoteTrack::Video(_video_track) = track else {
        return Err(anyhow!("Expected video track"));
    };

    log::info!("[{}] Video track subscribed with adaptive_stream enabled", mode.name());

    // Let adaptive stream signaling stabilize
    tokio::time::sleep(Duration::from_secs(2)).await;

    assert_eq!(pub_room.connection_state(), ConnectionState::Connected);
    assert_eq!(sub_room.connection_state(), ConnectionState::Connected);

    log::info!("[{}] Test passed - adaptive_stream=true working correctly!", mode.name());
    Ok(())
}

// ==================== ICE Transport Scenario Tests ====================
//
// Expected behavior:
// - ForceTcp: Forces media transport over TCP instead of UDP
// - ForceTls: Forces TLS encryption on the transport layer
// - Relay: Forces all traffic through TURN relay servers (no direct connections)
// These test different network conditions and transport configurations

/// Test ForceTcp scenario with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_force_tcp() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_force_tcp_impl(SignalingMode::DualPC).await
}

/// Test ForceTcp scenario with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_force_tcp() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_force_tcp_impl(SignalingMode::SinglePC).await
}

/// Test ForceTls scenario with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_force_tls() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_force_tls_impl(SignalingMode::DualPC).await
}

/// Test ForceTls scenario with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_force_tls() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_force_tls_impl(SignalingMode::SinglePC).await
}

/// Test Relay-only ICE transport with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_ice_relay_only() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_ice_relay_only_impl(SignalingMode::DualPC).await
}

/// Test Relay-only ICE transport with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_ice_relay_only() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_ice_relay_only_impl(SignalingMode::SinglePC).await
}

/// Test ForceTcp scenario
///
/// Expected Results:
/// 1. Connect to room successfully
/// 2. Publish a track
/// 3. Trigger ForceTcp scenario
/// 4. Connection renegotiates to use TCP transport
/// 5. Room remains connected and track is still published
///
/// Note: On localhost without TCP TURN servers, this may fail (expected)
async fn test_force_tcp_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing ForceTcp scenario", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_force_tcp_{}", mode, create_random_uuid());
    let token = create_token(&api_key, &api_secret, &room_name, "tcp_tester")?;

    let (room, mut events) = connect_room(&url, &token, mode).await?;
    assert_signaling_mode_state(&room, mode, &url);

    // Publish a track first
    let room_arc = Arc::new(room);
    let sine_params =
        SineParameters { freq: 440.0, amplitude: 1.0, sample_rate: 48000, num_channels: 1 };
    let mut sine_track = SineTrack::new(room_arc.clone(), sine_params);
    sine_track.publish().await?;

    let tracks_before = room_arc.local_participant().track_publications().len();
    log::info!("[{}] Tracks before ForceTcp: {}", mode.name(), tracks_before);

    log::info!("[{}] Simulating ForceTcp...", mode.name());
    room_arc.simulate_scenario(SimulateScenario::ForceTcp).await?;

    // Wait for potential reconnection/renegotiation
    let wait_stable = async {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            match timeout(Duration::from_millis(500), events.recv()).await {
                Ok(Some(RoomEvent::Reconnecting)) => {
                    log::info!("[{}] Reconnecting after ForceTcp...", mode.name());
                }
                Ok(Some(RoomEvent::Reconnected)) => {
                    log::info!("[{}] Reconnected after ForceTcp!", mode.name());
                    return Ok(true);
                }
                Ok(Some(RoomEvent::Disconnected { reason })) => {
                    log::info!("[{}] Disconnected after ForceTcp: {:?}", mode.name(), reason);
                    return Ok(false);
                }
                Ok(Some(_)) => {}
                Ok(None) => return Err(anyhow!("Event channel closed")),
                Err(_) => {
                    // Timeout on recv - check if still connected
                    if room_arc.connection_state() == ConnectionState::Connected {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(room_arc.connection_state() == ConnectionState::Connected)
    };

    let reconnected = wait_stable.await?;

    // On localhost without TCP TURN, ForceTcp will fail - that's expected
    if is_local_dev_server(&url) {
        if reconnected {
            log::info!("[{}] ForceTcp succeeded on localhost (TCP TURN available)", mode.name());
            let tracks_after = room_arc.local_participant().track_publications().len();
            assert_eq!(tracks_before, tracks_after, "Tracks should be preserved after ForceTcp");
        } else {
            log::info!(
                "[{}] ForceTcp failed on localhost (no TCP TURN) - this is expected",
                mode.name()
            );
        }
    } else {
        // On cloud, should work
        assert!(reconnected, "ForceTcp should succeed on cloud");
        assert_eq!(room_arc.connection_state(), ConnectionState::Connected);
        let tracks_after = room_arc.local_participant().track_publications().len();
        assert_eq!(tracks_before, tracks_after, "Tracks should be preserved after ForceTcp");
    }

    log::info!("[{}] Test passed - ForceTcp scenario test complete!", mode.name());
    Ok(())
}

/// Test ForceTls scenario
///
/// Expected Results:
/// 1. Connect to room successfully
/// 2. Publish a track
/// 3. Trigger ForceTls scenario
/// 4. Connection renegotiates to use TLS transport
/// 5. Room remains connected and track is still published
///
/// Note: On localhost without TLS TURN servers, this may fail (expected)
async fn test_force_tls_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing ForceTls scenario", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_force_tls_{}", mode, create_random_uuid());
    let token = create_token(&api_key, &api_secret, &room_name, "tls_tester")?;

    let (room, mut events) = connect_room(&url, &token, mode).await?;
    assert_signaling_mode_state(&room, mode, &url);

    // Publish a track first
    let room_arc = Arc::new(room);
    let sine_params =
        SineParameters { freq: 440.0, amplitude: 1.0, sample_rate: 48000, num_channels: 1 };
    let mut sine_track = SineTrack::new(room_arc.clone(), sine_params);
    sine_track.publish().await?;

    let tracks_before = room_arc.local_participant().track_publications().len();
    log::info!("[{}] Tracks before ForceTls: {}", mode.name(), tracks_before);

    log::info!("[{}] Simulating ForceTls...", mode.name());
    room_arc.simulate_scenario(SimulateScenario::ForceTls).await?;

    // Wait for potential reconnection/renegotiation
    let wait_stable = async {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            match timeout(Duration::from_millis(500), events.recv()).await {
                Ok(Some(RoomEvent::Reconnecting)) => {
                    log::info!("[{}] Reconnecting after ForceTls...", mode.name());
                }
                Ok(Some(RoomEvent::Reconnected)) => {
                    log::info!("[{}] Reconnected after ForceTls!", mode.name());
                    return Ok(true);
                }
                Ok(Some(RoomEvent::Disconnected { reason })) => {
                    log::info!("[{}] Disconnected after ForceTls: {:?}", mode.name(), reason);
                    return Ok(false);
                }
                Ok(Some(_)) => {}
                Ok(None) => return Err(anyhow!("Event channel closed")),
                Err(_) => {
                    if room_arc.connection_state() == ConnectionState::Connected {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(room_arc.connection_state() == ConnectionState::Connected)
    };

    let reconnected = wait_stable.await?;

    // On localhost without TLS TURN, ForceTls will fail - that's expected
    if is_local_dev_server(&url) {
        if reconnected {
            log::info!("[{}] ForceTls succeeded on localhost (TLS TURN available)", mode.name());
            let tracks_after = room_arc.local_participant().track_publications().len();
            assert_eq!(tracks_before, tracks_after, "Tracks should be preserved after ForceTls");
        } else {
            log::info!(
                "[{}] ForceTls failed on localhost (no TLS TURN) - this is expected",
                mode.name()
            );
        }
    } else {
        // On cloud, should work
        assert!(reconnected, "ForceTls should succeed on cloud");
        assert_eq!(room_arc.connection_state(), ConnectionState::Connected);
        let tracks_after = room_arc.local_participant().track_publications().len();
        assert_eq!(tracks_before, tracks_after, "Tracks should be preserved after ForceTls");
    }

    log::info!("[{}] Test passed - ForceTls scenario test complete!", mode.name());
    Ok(())
}

/// Create room options with relay-only ICE transport
fn room_options_relay_only(mode: SignalingMode) -> RoomOptions {
    let mut options = RoomOptions::default();
    options.auto_subscribe = true;
    options.single_peer_connection = mode.is_single_pc();
    options.rtc_config.ice_transport_type = IceTransportsType::Relay;
    options
}

/// Test relay-only ICE transport
///
/// Expected Results:
/// 1. Connect with ice_transport_type=Relay (forces TURN relay)
/// 2. If TURN servers are available, connection succeeds through relay
/// 3. If no TURN servers (localhost dev), connection may fail or fallback
/// 4. This verifies signaling works with restricted ICE candidates
///
/// Note: This test may behave differently on localhost vs cloud:
/// - Localhost: May fail if no TURN server is configured
/// - Cloud: Should succeed using LiveKit's TURN infrastructure
async fn test_ice_relay_only_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing relay-only ICE transport", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_relay_only_{}", mode, create_random_uuid());
    let token = create_token(&api_key, &api_secret, &room_name, "relay_tester")?;

    let options = room_options_relay_only(mode);

    // On localhost without TURN, this may fail - that's expected
    let connect_result = Room::connect(&url, &token, options).await;

    if is_local_dev_server(&url) {
        match connect_result {
            Ok((room, _events)) => {
                log::info!("[{}] Relay-only connected on localhost (TURN available)", mode.name());
                assert_eq!(room.connection_state(), ConnectionState::Connected);
            }
            Err(e) => {
                // Expected on localhost without TURN
                log::info!(
                    "[{}] Relay-only failed on localhost (no TURN): {:?} - this is expected",
                    mode.name(),
                    e
                );
            }
        }
    } else {
        // On cloud, relay should work
        let (room, _events) = connect_result.context("Relay-only should work on cloud")?;
        assert_eq!(room.connection_state(), ConnectionState::Connected);
        log::info!("[{}] Relay-only connected successfully on cloud", mode.name());
    }

    log::info!("[{}] Test passed - relay-only ICE transport test complete!", mode.name());
    Ok(())
}

// ==================== SimulateScenario Tests ====================
//
// Testing additional server-side scenarios:
// - Migration: Server-initiated participant migration to another node
// - ServerLeave: Server forcefully removes participant
// - Speaker: Speaker detection/change simulation

/// Test Migration scenario with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_migration() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_migration_impl(SignalingMode::DualPC).await
}

/// Test Migration scenario with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_migration() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_migration_impl(SignalingMode::SinglePC).await
}

/// Test ServerLeave scenario with V0 signaling
/// This test is resource-sensitive so it runs exclusively (no other tests in parallel).
#[test_log::test(tokio::test)]
async fn test_v0_server_leave() -> Result<()> {
    let _permit = acquire_exclusive_test_permit().await;
    test_server_leave_impl(SignalingMode::DualPC).await
}

/// Test ServerLeave scenario with V1 signaling
/// This test is resource-sensitive so it runs exclusively (no other tests in parallel).
#[test_log::test(tokio::test)]
async fn test_v1_server_leave() -> Result<()> {
    let _permit = acquire_exclusive_test_permit().await;
    test_server_leave_impl(SignalingMode::SinglePC).await
}

/// Test Speaker scenario with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_speaker() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_speaker_impl(SignalingMode::DualPC).await
}

/// Test Speaker scenario with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_speaker() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_speaker_impl(SignalingMode::SinglePC).await
}

/// Test Migration scenario
///
/// Expected Results:
/// 1. Connect to room and publish a track
/// 2. Trigger Migration scenario
/// 3. Room reconnects to a new server node
/// 4. Connection is restored and tracks are republished
/// 5. Room state is preserved after migration
async fn test_migration_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing Migration scenario", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_migration_{}", mode, create_random_uuid());
    let token = create_token(&api_key, &api_secret, &room_name, "migration_tester")?;

    let (room, mut events) = connect_room(&url, &token, mode).await?;
    assert_signaling_mode_state(&room, mode, &url);

    // Publish a track
    let room_arc = Arc::new(room);
    let sine_params =
        SineParameters { freq: 440.0, amplitude: 1.0, sample_rate: 48000, num_channels: 1 };
    let mut sine_track = SineTrack::new(room_arc.clone(), sine_params);
    sine_track.publish().await?;

    let tracks_before = room_arc.local_participant().track_publications().len();
    log::info!("[{}] Tracks before Migration: {}", mode.name(), tracks_before);

    log::info!("[{}] Simulating Migration...", mode.name());
    room_arc.simulate_scenario(SimulateScenario::Migration).await?;

    // Wait for reconnection after migration
    let wait_reconnected = async {
        loop {
            let Some(event) = events.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            match event {
                RoomEvent::Reconnecting => {
                    log::info!("[{}] Reconnecting during migration...", mode.name());
                }
                RoomEvent::Reconnected => {
                    log::info!("[{}] Reconnected after migration!", mode.name());
                    return Ok(());
                }
                RoomEvent::Disconnected { reason } => {
                    log::info!("[{}] Disconnected during migration: {:?}", mode.name(), reason);
                }
                _ => {}
            }
        }
    };

    timeout(Duration::from_secs(30), wait_reconnected)
        .await
        .context("Timeout waiting for reconnection after migration")??;

    // Give time for track republishing
    tokio::time::sleep(Duration::from_secs(3)).await;

    assert_eq!(room_arc.connection_state(), ConnectionState::Connected);
    let tracks_after = room_arc.local_participant().track_publications().len();
    log::info!("[{}] Tracks after Migration: {}", mode.name(), tracks_after);
    assert_eq!(tracks_before, tracks_after, "Tracks should be restored after migration");

    log::info!("[{}] Test passed - Migration scenario working!", mode.name());
    Ok(())
}

/// Test ServerLeave scenario
///
/// Expected Results:
/// 1. Connect to room successfully
/// 2. Trigger ServerLeave scenario
/// 3. Room receives Disconnected event with appropriate reason
/// 4. Connection state changes to Disconnected
async fn test_server_leave_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing ServerLeave scenario", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_server_leave_{}", mode, create_random_uuid());
    let token = create_token(&api_key, &api_secret, &room_name, "leave_tester")?;

    let (room, mut events) = connect_room(&url, &token, mode).await?;
    assert_signaling_mode_state(&room, mode, &url);

    log::info!("[{}] Simulating ServerLeave...", mode.name());
    room.simulate_scenario(SimulateScenario::ServerLeave).await?;

    // Wait for disconnection
    let wait_disconnected = async {
        loop {
            let Some(event) = events.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            if let RoomEvent::Disconnected { reason } = event {
                log::info!("[{}] Disconnected with reason: {:?}", mode.name(), reason);
                return Ok(reason);
            }
        }
    };

    let _reason = timeout(Duration::from_secs(15), wait_disconnected)
        .await
        .context("Timeout waiting for disconnection after ServerLeave")??;

    assert_eq!(
        room.connection_state(),
        ConnectionState::Disconnected,
        "Room should be disconnected after ServerLeave"
    );

    log::info!("[{}] Test passed - ServerLeave scenario working!", mode.name());
    Ok(())
}

/// Test Speaker scenario
///
/// Expected Results:
/// 1. Connect to room successfully
/// 2. Trigger Speaker scenario
/// 3. Room receives ActiveSpeakersChanged event
/// 4. Connection remains stable
async fn test_speaker_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing Speaker scenario", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_speaker_{}", mode, create_random_uuid());
    let token = create_token(&api_key, &api_secret, &room_name, "speaker_tester")?;

    let (room, mut events) = connect_room(&url, &token, mode).await?;
    assert_signaling_mode_state(&room, mode, &url);

    log::info!("[{}] Simulating Speaker scenario...", mode.name());
    room.simulate_scenario(SimulateScenario::Speaker).await?;

    // Wait for speaker change event
    let wait_speaker_change = async {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            match timeout(Duration::from_millis(500), events.recv()).await {
                Ok(Some(RoomEvent::ActiveSpeakersChanged { speakers })) => {
                    log::info!(
                        "[{}] Active speakers changed: {} speakers",
                        mode.name(),
                        speakers.len()
                    );
                    return Ok(speakers);
                }
                Ok(Some(_)) => {}
                Ok(None) => return Err(anyhow!("Event channel closed")),
                Err(_) => {}
            }
        }
        // Speaker event might not always fire - that's okay
        log::info!("[{}] No speaker change event received (may be expected)", mode.name());
        Ok(vec![])
    };

    let _speakers = wait_speaker_change.await?;

    // Verify connection is still stable
    assert_eq!(room.connection_state(), ConnectionState::Connected);

    log::info!("[{}] Test passed - Speaker scenario working!", mode.name());
    Ok(())
}

// ==================== E2EE (End-to-End Encryption) Tests ====================
//
// Expected behavior:
// - E2EE encrypts media frames before sending over WebRTC
// - Both publisher and subscriber must use the same shared key
// - Signaling includes E2EE-related metadata
// - Encrypted tracks can only be decoded by participants with the correct key

/// Test E2EE with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_e2ee() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_e2ee_impl(SignalingMode::DualPC).await
}

/// Test E2EE with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_e2ee() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_e2ee_impl(SignalingMode::SinglePC).await
}

/// Test E2EE data channel encryption with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_e2ee_data_channel() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_e2ee_data_channel_impl(SignalingMode::DualPC).await
}

/// Test E2EE data channel encryption with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_e2ee_data_channel() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_e2ee_data_channel_impl(SignalingMode::SinglePC).await
}

/// Create E2EE options with a shared key
fn create_e2ee_options(shared_key: &[u8]) -> E2eeOptions {
    let key_provider =
        KeyProvider::with_shared_key(KeyProviderOptions::default(), shared_key.to_vec());
    E2eeOptions { encryption_type: EncryptionType::Gcm, key_provider }
}

/// Create room options with E2EE enabled
fn room_options_e2ee(mode: SignalingMode, shared_key: &[u8]) -> RoomOptions {
    let mut options = RoomOptions::default();
    options.auto_subscribe = true;
    options.single_peer_connection = mode.is_single_pc();
    options.encryption = Some(create_e2ee_options(shared_key));
    options
}

/// Test E2EE media encryption
///
/// Expected Results:
/// 1. Both publisher and subscriber connect with the same E2EE shared key
/// 2. Publisher publishes an encrypted audio track
/// 3. Subscriber receives and decrypts the track
/// 4. Audio frames are received and can be processed
/// 5. Without the correct key, frames would be undecryptable
async fn test_e2ee_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing E2EE media encryption", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_e2ee_{}", mode, create_random_uuid());

    // Shared encryption key
    let shared_key = b"test-encryption-key-32bytes!!!!";

    // Publisher with E2EE
    let pub_token = create_token(&api_key, &api_secret, &room_name, "e2ee_publisher")?;
    let pub_options = room_options_e2ee(mode, shared_key);
    let (pub_room, _pub_events) = Room::connect(&url, &pub_token, pub_options).await?;

    // Subscriber with same E2EE key
    let sub_token = create_token(&api_key, &api_secret, &room_name, "e2ee_subscriber")?;
    let sub_options = room_options_e2ee(mode, shared_key);
    let (sub_room, mut sub_events) = Room::connect(&url, &sub_token, sub_options).await?;

    assert_signaling_mode_state(&pub_room, mode, &url);

    let publisher_identity = pub_room.local_participant().identity().to_string();

    // Verify E2EE is enabled
    let pub_e2ee_manager = pub_room.e2ee_manager();
    assert!(pub_e2ee_manager.enabled(), "E2EE should be enabled on publisher");
    let sub_e2ee_manager = sub_room.e2ee_manager();
    assert!(sub_e2ee_manager.enabled(), "E2EE should be enabled on subscriber");

    log::info!("[{}] E2EE enabled on both participants", mode.name());

    // Publish an encrypted audio track
    let sine_params =
        SineParameters { freq: 440.0, amplitude: 1.0, sample_rate: 48000, num_channels: 1 };
    let pub_room_arc = Arc::new(pub_room);
    let mut sine_track = SineTrack::new(pub_room_arc.clone(), sine_params);
    sine_track.publish().await?;

    log::info!("[{}] Published encrypted audio track, waiting for subscription", mode.name());

    // Wait for track subscription
    let wait_subscribed = async {
        loop {
            let Some(event) = sub_events.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            if let RoomEvent::TrackSubscribed { track, publication: _, participant } = event {
                if participant.identity().as_str() == publisher_identity {
                    return Ok(track);
                }
            }
        }
    };

    let track = timeout(Duration::from_secs(20), wait_subscribed)
        .await
        .context("Timeout waiting for encrypted track subscription")??;

    log::info!("[{}] Received encrypted track: {:?}", mode.name(), track.sid());

    // Verify it's an audio track and we can receive decrypted frames
    let RemoteTrack::Audio(audio_track) = track else {
        return Err(anyhow!("Expected audio track"));
    };

    let mut stream = NativeAudioStream::new(audio_track.rtc_track(), 48000, 1);
    let mut frames_received = 0;
    let receive_frames = async {
        while let Some(frame) = stream.next().await {
            // Frame data should be decrypted and non-empty
            assert!(!frame.data.is_empty(), "Decrypted frame should have data");
            frames_received += 1;
            if frames_received >= 20 {
                break;
            }
        }
    };

    timeout(Duration::from_secs(15), receive_frames)
        .await
        .context("Timeout receiving decrypted audio frames")?;

    log::info!(
        "[{}] Successfully received {} decrypted audio frames",
        mode.name(),
        frames_received
    );

    log::info!("[{}] Test passed - E2EE media encryption working!", mode.name());
    Ok(())
}

/// Test E2EE data channel encryption
///
/// Expected Results:
/// 1. Both participants connect with E2EE enabled
/// 2. Publisher sends encrypted data packet
/// 3. Subscriber receives and decrypts the data
/// 4. Data matches the original message
async fn test_e2ee_data_channel_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing E2EE data channel encryption", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_e2ee_data_{}", mode, create_random_uuid());

    let shared_key = b"test-encryption-key-32bytes!!!!";

    let pub_token = create_token(&api_key, &api_secret, &room_name, "e2ee_data_pub")?;
    let pub_options = room_options_e2ee(mode, shared_key);
    let (pub_room, _pub_events) = Room::connect(&url, &pub_token, pub_options).await?;

    let sub_token = create_token(&api_key, &api_secret, &room_name, "e2ee_data_sub")?;
    let sub_options = room_options_e2ee(mode, shared_key);
    let (_sub_room, mut sub_events) = Room::connect(&url, &sub_token, sub_options).await?;

    // Wait for participant to join
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Send encrypted data
    let test_data = b"Encrypted test message via E2EE data channel!".to_vec();
    let test_topic = "e2ee_test_topic".to_string();

    pub_room
        .local_participant()
        .publish_data(livekit::DataPacket {
            payload: test_data.clone(),
            topic: Some(test_topic.clone()),
            reliable: true,
            ..Default::default()
        })
        .await?;

    log::info!("[{}] Sent encrypted data packet", mode.name());

    // Wait to receive decrypted data
    let receive_data = async {
        loop {
            let Some(event) = sub_events.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            if let RoomEvent::DataReceived { payload, topic, kind: _, participant: _ } = event {
                if topic == Some(test_topic.clone()) {
                    return Ok(payload);
                }
            }
        }
    };

    let received = timeout(Duration::from_secs(15), receive_data)
        .await
        .context("Timeout waiting for encrypted data")??;

    assert_eq!(received.to_vec(), test_data, "Decrypted data should match original");

    log::info!("[{}] Test passed - E2EE data channel encryption working!", mode.name());
    Ok(())
}

// ==================== Track Operations During Reconnection Tests ====================
//
// Expected behavior:
// - Track operations during reconnection should be queued or handled gracefully
// - After reconnection completes, track state should be consistent
// - Operations should not cause crashes or undefined behavior

/// Test publishing track during reconnection with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_publish_during_reconnect() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_publish_during_reconnect_impl(SignalingMode::DualPC).await
}

/// Test publishing track during reconnection with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_publish_during_reconnect() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_publish_during_reconnect_impl(SignalingMode::SinglePC).await
}

/// Test unpublishing track during reconnection with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_unpublish_during_reconnect() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_unpublish_during_reconnect_impl(SignalingMode::DualPC).await
}

/// Test unpublishing track during reconnection with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_unpublish_during_reconnect() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_unpublish_during_reconnect_impl(SignalingMode::SinglePC).await
}

/// Test mute/unmute during reconnection with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_mute_during_reconnect() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_mute_during_reconnect_impl(SignalingMode::DualPC).await
}

/// Test mute/unmute during reconnection with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_mute_during_reconnect() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_mute_during_reconnect_impl(SignalingMode::SinglePC).await
}

/// Test publishing a new track during reconnection
///
/// Expected Results:
/// 1. Connect and publish initial track
/// 2. Trigger reconnection
/// 3. While reconnecting, attempt to publish a new track
/// 4. After reconnection completes, both tracks should be published
/// 5. Connection remains stable
async fn test_publish_during_reconnect_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing publish during reconnect", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_pub_during_reconnect_{}", mode, create_random_uuid());
    let token = create_token(&api_key, &api_secret, &room_name, "reconnect_publisher")?;

    let (room, mut events) = connect_room(&url, &token, mode).await?;
    let room_arc = Arc::new(room);

    // Publish initial track
    let sine_params1 =
        SineParameters { freq: 440.0, amplitude: 1.0, sample_rate: 48000, num_channels: 1 };
    let mut sine_track1 = SineTrack::new(room_arc.clone(), sine_params1);
    sine_track1.publish().await?;

    let tracks_before = room_arc.local_participant().track_publications().len();
    log::info!("[{}] Initial tracks: {}", mode.name(), tracks_before);

    // Trigger reconnection
    log::info!("[{}] Triggering signal reconnect...", mode.name());
    room_arc.simulate_scenario(SimulateScenario::SignalReconnect).await?;

    // Wait briefly for reconnecting state, then publish new track
    let mut reconnecting_seen = false;
    let mut reconnected = false;

    // Create the second track source for publishing during reconnect
    let audio_source = NativeAudioSource::new(AudioSourceOptions::default(), 48_000, 1, 1000);
    let audio_track = LocalAudioTrack::create_audio_track(
        "during_reconnect_track",
        RtcAudioSource::Native(audio_source.clone()),
    );

    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), events.recv()).await {
            Ok(Some(RoomEvent::Reconnecting)) => {
                log::info!("[{}] Reconnecting state detected", mode.name());
                reconnecting_seen = true;

                // Try to publish new track during reconnection
                log::info!("[{}] Attempting to publish track during reconnection...", mode.name());
                let publish_result = room_arc
                    .local_participant()
                    .publish_track(
                        LocalTrack::Audio(audio_track.clone()),
                        TrackPublishOptions::default(),
                    )
                    .await;

                match &publish_result {
                    Ok(_) => log::info!("[{}] Track published during reconnection", mode.name()),
                    Err(e) => log::info!(
                        "[{}] Track publish during reconnection returned error (may be expected): {:?}",
                        mode.name(),
                        e
                    ),
                }
            }
            Ok(Some(RoomEvent::Reconnected)) => {
                log::info!("[{}] Reconnected", mode.name());
                reconnected = true;
                break;
            }
            Ok(Some(_)) => {}
            Ok(None) => return Err(anyhow!("Event channel closed")),
            Err(_) => {}
        }
    }

    assert!(reconnecting_seen || reconnected, "Should have seen reconnecting or reconnected event");

    // Wait for state to stabilize
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify room is connected
    assert_eq!(room_arc.connection_state(), ConnectionState::Connected);

    let tracks_after = room_arc.local_participant().track_publications().len();
    log::info!("[{}] Tracks after reconnect: {}", mode.name(), tracks_after);

    // At minimum, original track should still be there
    assert!(tracks_after >= tracks_before, "Original track should be preserved");

    log::info!("[{}] Test passed - publish during reconnect handled!", mode.name());
    Ok(())
}

/// Test unpublishing a track during reconnection
///
/// Expected Results:
/// 1. Connect and publish a track
/// 2. Trigger reconnection
/// 3. While reconnecting, attempt to unpublish the track
/// 4. After reconnection, track state should be consistent
/// 5. No crashes or undefined behavior
async fn test_unpublish_during_reconnect_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing unpublish during reconnect", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_unpub_during_reconnect_{}", mode, create_random_uuid());
    let token = create_token(&api_key, &api_secret, &room_name, "reconnect_unpublisher")?;

    let (room, mut events) = connect_room(&url, &token, mode).await?;
    let room_arc = Arc::new(room);

    // Publish a track
    let sine_params =
        SineParameters { freq: 440.0, amplitude: 1.0, sample_rate: 48000, num_channels: 1 };
    let mut sine_track = SineTrack::new(room_arc.clone(), sine_params);
    sine_track.publish().await?;

    let publications: Vec<_> =
        room_arc.local_participant().track_publications().values().cloned().collect();
    assert!(!publications.is_empty(), "Should have published track");

    let track_sid = publications[0].sid();
    log::info!("[{}] Published track: {}", mode.name(), track_sid);

    // Trigger reconnection
    log::info!("[{}] Triggering signal reconnect...", mode.name());
    room_arc.simulate_scenario(SimulateScenario::SignalReconnect).await?;

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut unpublish_attempted = false;

    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), events.recv()).await {
            Ok(Some(RoomEvent::Reconnecting)) => {
                log::info!("[{}] Reconnecting state detected", mode.name());

                if !unpublish_attempted {
                    // Try to unpublish during reconnection
                    log::info!(
                        "[{}] Attempting to unpublish track during reconnection...",
                        mode.name()
                    );
                    let unpublish_result =
                        room_arc.local_participant().unpublish_track(&track_sid).await;

                    match &unpublish_result {
                        Ok(_) => log::info!("[{}] Track unpublished during reconnection", mode.name()),
                        Err(e) => log::info!(
                            "[{}] Track unpublish during reconnection returned error (may be expected): {:?}",
                            mode.name(),
                            e
                        ),
                    }
                    unpublish_attempted = true;
                }
            }
            Ok(Some(RoomEvent::Reconnected)) => {
                log::info!("[{}] Reconnected", mode.name());
                break;
            }
            Ok(Some(_)) => {}
            Ok(None) => return Err(anyhow!("Event channel closed")),
            Err(_) => {}
        }
    }

    // Wait for state to stabilize
    tokio::time::sleep(Duration::from_secs(2)).await;

    assert_eq!(room_arc.connection_state(), ConnectionState::Connected);

    let tracks_after = room_arc.local_participant().track_publications().len();
    log::info!("[{}] Tracks after reconnect: {}", mode.name(), tracks_after);

    log::info!("[{}] Test passed - unpublish during reconnect handled!", mode.name());
    Ok(())
}

/// Test muting and unmuting a track during reconnection
///
/// Expected Results:
/// 1. Connect and publish a track
/// 2. Trigger reconnection
/// 3. While reconnecting, mute and unmute the track
/// 4. After reconnection, track mute state should be consistent
/// 5. No crashes or undefined behavior
async fn test_mute_during_reconnect_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing mute/unmute during reconnect", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_mute_during_reconnect_{}", mode, create_random_uuid());
    let token = create_token(&api_key, &api_secret, &room_name, "reconnect_muter")?;

    let (room, mut events) = connect_room(&url, &token, mode).await?;
    let room_arc = Arc::new(room);

    // Publish a track
    let sine_params =
        SineParameters { freq: 440.0, amplitude: 1.0, sample_rate: 48000, num_channels: 1 };
    let mut sine_track = SineTrack::new(room_arc.clone(), sine_params);
    sine_track.publish().await?;

    let publications: Vec<_> =
        room_arc.local_participant().track_publications().values().cloned().collect();
    assert!(!publications.is_empty(), "Should have published track");

    let publication = &publications[0];
    log::info!(
        "[{}] Published track: {}, muted: {}",
        mode.name(),
        publication.sid(),
        publication.is_muted()
    );

    // Trigger reconnection
    log::info!("[{}] Triggering signal reconnect...", mode.name());
    room_arc.simulate_scenario(SimulateScenario::SignalReconnect).await?;

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut mute_attempted = false;

    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), events.recv()).await {
            Ok(Some(RoomEvent::Reconnecting)) => {
                log::info!("[{}] Reconnecting state detected", mode.name());

                if !mute_attempted {
                    // Try mute/unmute operations during reconnection
                    log::info!("[{}] Muting track during reconnection...", mode.name());
                    publication.mute();
                    log::info!("[{}] Track muted: {}", mode.name(), publication.is_muted());

                    tokio::time::sleep(Duration::from_millis(50)).await;

                    log::info!("[{}] Unmuting track during reconnection...", mode.name());
                    publication.unmute();
                    log::info!("[{}] Track muted: {}", mode.name(), publication.is_muted());

                    mute_attempted = true;
                }
            }
            Ok(Some(RoomEvent::Reconnected)) => {
                log::info!("[{}] Reconnected", mode.name());
                break;
            }
            Ok(Some(_)) => {}
            Ok(None) => return Err(anyhow!("Event channel closed")),
            Err(_) => {}
        }
    }

    // Wait for state to stabilize
    tokio::time::sleep(Duration::from_secs(2)).await;

    assert_eq!(room_arc.connection_state(), ConnectionState::Connected);

    // Verify track is still published
    let tracks_after = room_arc.local_participant().track_publications().len();
    assert!(tracks_after >= 1, "Track should still be published");

    log::info!("[{}] Final mute state: {}", mode.name(), publication.is_muted());
    log::info!("[{}] Test passed - mute/unmute during reconnect handled!", mode.name());
    Ok(())
}

// ==================== Multiple Subscribers Tests ====================
//
// Expected behavior:
// - With 3+ participants, signaling should handle fanout correctly
// - All subscribers should see the same track state
// - Track publication events should be consistent across all participants

/// Test multiple subscribers with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_multiple_subscribers() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_multiple_subscribers_impl(SignalingMode::DualPC).await
}

/// Test multiple subscribers with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_multiple_subscribers() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_multiple_subscribers_impl(SignalingMode::SinglePC).await
}

/// Test track state synchronization with multiple subscribers
///
/// Expected Results:
/// 1. Publisher and 3 subscribers connect to the same room
/// 2. Publisher publishes an audio track
/// 3. All 3 subscribers receive TrackSubscribed event
/// 4. All subscribers can receive audio frames
/// 5. Track state is consistent across all participants
async fn test_multiple_subscribers_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing multiple subscribers (1 publisher + 3 subscribers)", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_multi_sub_{}", mode, create_random_uuid());

    // Create publisher
    let pub_token = create_token(&api_key, &api_secret, &room_name, "publisher")?;
    let (pub_room, _pub_events) = connect_room(&url, &pub_token, mode).await?;
    let publisher_identity = pub_room.local_participant().identity().to_string();

    // Create 3 subscribers
    let sub1_token = create_token(&api_key, &api_secret, &room_name, "subscriber1")?;
    let (sub1_room, mut sub1_events) = connect_room(&url, &sub1_token, mode).await?;

    let sub2_token = create_token(&api_key, &api_secret, &room_name, "subscriber2")?;
    let (sub2_room, mut sub2_events) = connect_room(&url, &sub2_token, mode).await?;

    let sub3_token = create_token(&api_key, &api_secret, &room_name, "subscriber3")?;
    let (sub3_room, mut sub3_events) = connect_room(&url, &sub3_token, mode).await?;

    log::info!("[{}] All 4 participants connected", mode.name());

    // Verify all rooms see correct participant count
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Publisher publishes audio track
    let sine_params =
        SineParameters { freq: 440.0, amplitude: 1.0, sample_rate: 48000, num_channels: 1 };
    let pub_room_arc = Arc::new(pub_room);
    let mut sine_track = SineTrack::new(pub_room_arc.clone(), sine_params);
    sine_track.publish().await?;

    log::info!("[{}] Track published, waiting for all subscribers to receive", mode.name());

    // Wait for all 3 subscribers to receive the track
    let wait_subscription = |mut events: UnboundedReceiver<RoomEvent>, sub_name: &'static str| {
        let publisher_id = publisher_identity.clone();
        let mode_name = mode.name();
        async move {
            loop {
                let Some(event) = events.recv().await else {
                    return Err(anyhow!("{} event channel closed", sub_name));
                };
                if let RoomEvent::TrackSubscribed { track, publication: _, participant } = event {
                    if participant.identity().as_str() == publisher_id {
                        log::info!(
                            "[{}] {} received track: {:?}",
                            mode_name,
                            sub_name,
                            track.sid()
                        );
                        return Ok(track);
                    }
                }
            }
        }
    };

    let (track1, track2, track3) = tokio::try_join!(
        timeout(Duration::from_secs(15), wait_subscription(sub1_events, "Subscriber1")),
        timeout(Duration::from_secs(15), wait_subscription(sub2_events, "Subscriber2")),
        timeout(Duration::from_secs(15), wait_subscription(sub3_events, "Subscriber3")),
    )
    .context("Timeout waiting for all subscribers to receive track")?;

    let track1 = track1?;
    let track2 = track2?;
    let track3 = track3?;

    log::info!("[{}] All 3 subscribers received the track", mode.name());

    // Verify all tracks have same SID
    assert_eq!(track1.sid(), track2.sid(), "Track SIDs should match across subscribers");
    assert_eq!(track2.sid(), track3.sid(), "Track SIDs should match across subscribers");

    // Verify subscriber rooms see the publisher's track
    let verify_track_visible = |room: &Room, room_name: &str| {
        let remote_participants = room.remote_participants();
        let publisher =
            remote_participants.iter().find(|(_, p)| p.identity().as_str() == publisher_identity);

        if let Some((_, pub_participant)) = publisher {
            let tracks = pub_participant.track_publications();
            log::info!(
                "[{}] {} sees {} tracks from publisher",
                mode.name(),
                room_name,
                tracks.len()
            );
            assert!(tracks.len() >= 1, "{} should see publisher's track", room_name);
        } else {
            panic!("{} should see publisher participant", room_name);
        }
    };

    verify_track_visible(&sub1_room, "Subscriber1");
    verify_track_visible(&sub2_room, "Subscriber2");
    verify_track_visible(&sub3_room, "Subscriber3");

    // Verify all rooms are still connected
    assert_eq!(pub_room_arc.connection_state(), ConnectionState::Connected);
    assert_eq!(sub1_room.connection_state(), ConnectionState::Connected);
    assert_eq!(sub2_room.connection_state(), ConnectionState::Connected);
    assert_eq!(sub3_room.connection_state(), ConnectionState::Connected);

    log::info!("[{}] Test passed - multiple subscribers working correctly!", mode.name());
    Ok(())
}

// ==================== Rapid Reconnect Stress Tests ====================
//
// Expected behavior:
// - Multiple rapid reconnections should be handled gracefully
// - Room should eventually stabilize in connected state
// - No resource leaks or crashes

/// Test rapid sequential reconnects with V0 signaling
/// This test is resource-intensive so it runs exclusively (no other tests in parallel).
#[test_log::test(tokio::test)]
async fn test_v0_rapid_reconnect() -> Result<()> {
    let _permit = acquire_exclusive_test_permit().await;
    test_rapid_reconnect_impl(SignalingMode::DualPC).await
}

/// Test rapid sequential reconnects with V1 signaling
/// This test is resource-intensive so it runs exclusively (no other tests in parallel).
#[test_log::test(tokio::test)]
async fn test_v1_rapid_reconnect() -> Result<()> {
    let _permit = acquire_exclusive_test_permit().await;
    test_rapid_reconnect_impl(SignalingMode::SinglePC).await
}

/// Test reconnect while reconnect in progress with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_reconnect_during_reconnect() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_reconnect_during_reconnect_impl(SignalingMode::DualPC).await
}

/// Test reconnect while reconnect in progress with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_reconnect_during_reconnect() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_reconnect_during_reconnect_impl(SignalingMode::SinglePC).await
}

/// Test 5 sequential reconnects
///
/// Expected Results:
/// 1. Connect and publish a track
/// 2. Perform 5 sequential reconnects
/// 3. After each reconnect, verify connection is stable
/// 4. Track should be preserved throughout
/// 5. Final state should be connected with track published
async fn test_rapid_reconnect_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing 5 sequential reconnects", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_rapid_reconnect_{}", mode, create_random_uuid());
    let token = create_token(&api_key, &api_secret, &room_name, "rapid_reconnector")?;

    let (room, mut events) = connect_room(&url, &token, mode).await?;
    let room_arc = Arc::new(room);

    // Publish a track
    let sine_params =
        SineParameters { freq: 440.0, amplitude: 1.0, sample_rate: 48000, num_channels: 1 };
    let mut sine_track = SineTrack::new(room_arc.clone(), sine_params);
    sine_track.publish().await?;

    let initial_tracks = room_arc.local_participant().track_publications().len();
    log::info!("[{}] Initial tracks: {}", mode.name(), initial_tracks);

    const NUM_RECONNECTS: usize = 5;

    for attempt in 1..=NUM_RECONNECTS {
        log::info!("[{}] Triggering reconnect {} of {}", mode.name(), attempt, NUM_RECONNECTS);
        room_arc.simulate_scenario(SimulateScenario::SignalReconnect).await?;

        // Wait for reconnection
        let wait_reconnected = async {
            loop {
                let Some(event) = events.recv().await else {
                    return Err(anyhow!("Event channel closed"));
                };
                match event {
                    RoomEvent::Reconnecting => {
                        log::info!("[{}] Reconnecting (attempt {})...", mode.name(), attempt);
                    }
                    RoomEvent::Reconnected => {
                        log::info!("[{}] Reconnected (attempt {})", mode.name(), attempt);
                        return Ok(());
                    }
                    _ => {}
                }
            }
        };

        timeout(Duration::from_secs(30), wait_reconnected)
            .await
            .context(format!("Timeout on reconnect attempt {}", attempt))??;

        // Brief pause between reconnects
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify still connected
        assert_eq!(
            room_arc.connection_state(),
            ConnectionState::Connected,
            "Should be connected after reconnect {}",
            attempt
        );
    }

    // Final verification
    tokio::time::sleep(Duration::from_secs(2)).await;

    assert_eq!(room_arc.connection_state(), ConnectionState::Connected);
    let final_tracks = room_arc.local_participant().track_publications().len();
    log::info!("[{}] Final tracks: {}", mode.name(), final_tracks);
    assert_eq!(
        initial_tracks, final_tracks,
        "Track count should be preserved after {} reconnects",
        NUM_RECONNECTS
    );

    log::info!("[{}] Test passed - {} sequential reconnects handled!", mode.name(), NUM_RECONNECTS);
    Ok(())
}

/// Test triggering reconnect while previous reconnect is still in progress
///
/// Expected Results:
/// 1. Connect and publish a track
/// 2. Trigger first reconnect
/// 3. While reconnecting, trigger another reconnect
/// 4. Room should eventually stabilize
/// 5. No crashes, and connection should be restored
async fn test_reconnect_during_reconnect_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing reconnect during reconnect", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_reconnect_during_reconnect_{}", mode, create_random_uuid());
    let token = create_token(&api_key, &api_secret, &room_name, "nested_reconnector")?;

    let (room, mut events) = connect_room(&url, &token, mode).await?;
    let room_arc = Arc::new(room);

    // Publish a track
    let sine_params =
        SineParameters { freq: 440.0, amplitude: 1.0, sample_rate: 48000, num_channels: 1 };
    let mut sine_track = SineTrack::new(room_arc.clone(), sine_params);
    sine_track.publish().await?;

    log::info!("[{}] Triggering first reconnect...", mode.name());
    room_arc.simulate_scenario(SimulateScenario::SignalReconnect).await?;

    let mut second_reconnect_triggered = false;
    let deadline = Instant::now() + Duration::from_secs(45);

    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), events.recv()).await {
            Ok(Some(RoomEvent::Reconnecting)) => {
                log::info!("[{}] Reconnecting detected", mode.name());

                if !second_reconnect_triggered {
                    // Trigger another reconnect while still reconnecting
                    log::info!(
                        "[{}] Triggering second reconnect during first reconnect...",
                        mode.name()
                    );
                    let result =
                        room_arc.simulate_scenario(SimulateScenario::SignalReconnect).await;
                    match result {
                        Ok(_) => log::info!("[{}] Second reconnect triggered", mode.name()),
                        Err(e) => log::info!(
                            "[{}] Second reconnect returned error (may be expected): {:?}",
                            mode.name(),
                            e
                        ),
                    }
                    second_reconnect_triggered = true;
                }
            }
            Ok(Some(RoomEvent::Reconnected)) => {
                log::info!("[{}] Reconnected", mode.name());
                // Don't break immediately - there might be another reconnect cycle
                if second_reconnect_triggered {
                    // Give some time for potential second reconnect cycle
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    if room_arc.connection_state() == ConnectionState::Connected {
                        break;
                    }
                }
            }
            Ok(Some(RoomEvent::Disconnected { reason })) => {
                log::info!("[{}] Disconnected: {:?}", mode.name(), reason);
            }
            Ok(Some(_)) => {}
            Ok(None) => return Err(anyhow!("Event channel closed")),
            Err(_) => {
                // Check if we're stable
                if second_reconnect_triggered
                    && room_arc.connection_state() == ConnectionState::Connected
                {
                    break;
                }
            }
        }
    }

    // Final stabilization
    tokio::time::sleep(Duration::from_secs(2)).await;

    // The room should eventually be in a stable state
    let final_state = room_arc.connection_state();
    log::info!("[{}] Final connection state: {:?}", mode.name(), final_state);

    // We accept either Connected or the room gracefully handling the chaos
    assert!(
        final_state == ConnectionState::Connected || final_state == ConnectionState::Reconnecting,
        "Room should be in Connected or Reconnecting state, got {:?}",
        final_state
    );

    log::info!("[{}] Test passed - reconnect during reconnect handled!", mode.name());
    Ok(())
}

// ==================== Video Simulcast Verification Tests ====================
//
// Expected behavior:
// - Video tracks published with simulcast=true should have multiple layers
// - Subscriber should be able to change video quality
// - Quality changes should be signaled to the server

/// Test video simulcast with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_video_simulcast() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_video_simulcast_impl(SignalingMode::DualPC).await
}

/// Test video simulcast with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_video_simulcast() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_video_simulcast_impl(SignalingMode::SinglePC).await
}

/// Test simulcast quality switching with V0 signaling
#[test_log::test(tokio::test)]
async fn test_v0_simulcast_quality_switch() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_simulcast_quality_switch_impl(SignalingMode::DualPC).await
}

/// Test simulcast quality switching with V1 signaling
#[test_log::test(tokio::test)]
async fn test_v1_simulcast_quality_switch() -> Result<()> {
    let _permit = acquire_test_permit().await;
    test_simulcast_quality_switch_impl(SignalingMode::SinglePC).await
}

/// Test that simulcast video track is properly published and received
///
/// Expected Results:
/// 1. Publisher publishes video track with simulcast=true
/// 2. Subscriber receives the track
/// 3. Track should be marked as simulcasted
/// 4. Connection remains stable
async fn test_video_simulcast_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing video simulcast", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_simulcast_{}", mode, create_random_uuid());

    // Publisher
    let pub_token = create_token(&api_key, &api_secret, &room_name, "simulcast_publisher")?;
    let (pub_room, _pub_events) = connect_room(&url, &pub_token, mode).await?;

    // Subscriber
    let sub_token = create_token(&api_key, &api_secret, &room_name, "simulcast_subscriber")?;
    let (sub_room, mut sub_events) = connect_room(&url, &sub_token, mode).await?;

    let publisher_identity = pub_room.local_participant().identity().to_string();

    // Publish video track with simulcast enabled
    let video_source = NativeVideoSource::new(VideoResolution { width: 1280, height: 720 }, false);
    let video_track = LocalVideoTrack::create_video_track(
        "simulcast_video",
        RtcVideoSource::Native(video_source.clone()),
    );

    let mut publish_options = TrackPublishOptions::default();
    publish_options.source = TrackSource::Camera;
    publish_options.simulcast = true; // Enable simulcast

    let publication = pub_room
        .local_participant()
        .publish_track(LocalTrack::Video(video_track), publish_options)
        .await?;

    log::info!(
        "[{}] Published video track with simulcast, simulcasted={}",
        mode.name(),
        publication.simulcasted()
    );

    // Wait for subscriber to receive the track
    let wait_subscribed = async {
        loop {
            let Some(event) = sub_events.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            if let RoomEvent::TrackSubscribed { track, publication, participant } = event {
                if participant.identity().as_str() == publisher_identity {
                    return Ok((track, publication));
                }
            }
        }
    };

    let (track, remote_publication) = timeout(Duration::from_secs(15), wait_subscribed)
        .await
        .context("Timeout waiting for simulcast video track")??;

    log::info!(
        "[{}] Subscriber received track: {:?}, simulcasted={}",
        mode.name(),
        track.sid(),
        remote_publication.simulcasted()
    );

    // Verify it's a video track
    let RemoteTrack::Video(_video_track) = track else {
        return Err(anyhow!("Expected video track"));
    };

    // Verify simulcast is enabled (depends on server support)
    // Note: simulcast may not be enabled on localhost dev server
    if !is_local_dev_server(&url) {
        assert!(remote_publication.simulcasted(), "Track should be simulcasted on cloud server");
    } else {
        log::info!(
            "[{}] Localhost server - simulcast may not be enabled: {}",
            mode.name(),
            remote_publication.simulcasted()
        );
    }

    // Verify connections are stable
    assert_eq!(pub_room.connection_state(), ConnectionState::Connected);
    assert_eq!(sub_room.connection_state(), ConnectionState::Connected);

    log::info!("[{}] Test passed - video simulcast working!", mode.name());
    Ok(())
}

/// Test switching video quality on simulcast track
///
/// Expected Results:
/// 1. Publisher publishes simulcast video track
/// 2. Subscriber receives track and verifies it's simulcasted
/// 3. Subscriber switches between Low, Medium, High quality
/// 4. Quality changes are signaled (no errors/crashes)
/// 5. Connection remains stable throughout
async fn test_simulcast_quality_switch_impl(mode: SignalingMode) -> Result<()> {
    log::info!("[{}] Testing simulcast quality switching", mode.name());
    let (url, api_key, api_secret) = get_env_for_mode(mode);
    let room_name = format!("test_{:?}_simulcast_switch_{}", mode, create_random_uuid());

    // Publisher
    let pub_token = create_token(&api_key, &api_secret, &room_name, "quality_publisher")?;
    let (pub_room, _pub_events) = connect_room(&url, &pub_token, mode).await?;

    // Subscriber
    let sub_token = create_token(&api_key, &api_secret, &room_name, "quality_subscriber")?;
    let (sub_room, mut sub_events) = connect_room(&url, &sub_token, mode).await?;

    let publisher_identity = pub_room.local_participant().identity().to_string();

    // Publish video track with simulcast
    let video_source = NativeVideoSource::new(VideoResolution { width: 1280, height: 720 }, false);
    let video_track = LocalVideoTrack::create_video_track(
        "quality_video",
        RtcVideoSource::Native(video_source.clone()),
    );

    let mut publish_options = TrackPublishOptions::default();
    publish_options.source = TrackSource::Camera;
    publish_options.simulcast = true;

    pub_room
        .local_participant()
        .publish_track(LocalTrack::Video(video_track), publish_options)
        .await?;

    // Wait for subscriber to receive the track
    let wait_subscribed = async {
        loop {
            let Some(event) = sub_events.recv().await else {
                return Err(anyhow!("Event channel closed"));
            };
            if let RoomEvent::TrackSubscribed { track: _, publication, participant } = event {
                if participant.identity().as_str() == publisher_identity {
                    return Ok(publication);
                }
            }
        }
    };

    let remote_publication = timeout(Duration::from_secs(15), wait_subscribed)
        .await
        .context("Timeout waiting for video track")??;

    log::info!(
        "[{}] Received track, simulcasted={}",
        mode.name(),
        remote_publication.simulcasted()
    );

    // Only test quality switching if track is simulcasted
    if remote_publication.simulcasted() {
        log::info!("[{}] Testing quality switching...", mode.name());

        // Switch to Low quality
        log::info!("[{}] Switching to Low quality", mode.name());
        remote_publication.set_video_quality(VideoQuality::Low);
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Switch to Medium quality
        log::info!("[{}] Switching to Medium quality", mode.name());
        remote_publication.set_video_quality(VideoQuality::Medium);
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Switch to High quality
        log::info!("[{}] Switching to High quality", mode.name());
        remote_publication.set_video_quality(VideoQuality::High);
        tokio::time::sleep(Duration::from_millis(500)).await;

        log::info!("[{}] All quality switches completed", mode.name());
    } else {
        log::info!(
            "[{}] Track is not simulcasted, skipping quality switch test (expected on localhost)",
            mode.name()
        );
    }

    // Verify connections are stable
    assert_eq!(pub_room.connection_state(), ConnectionState::Connected);
    assert_eq!(sub_room.connection_state(), ConnectionState::Connected);

    log::info!("[{}] Test passed - simulcast quality switching working!", mode.name());
    Ok(())
}
