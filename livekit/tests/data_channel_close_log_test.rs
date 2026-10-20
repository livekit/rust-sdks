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

//! Publisher data channel close reporting
//!
//! Tearing a session down from the server closes the publisher data channels from the remote
//! side. That close is expected and must not be reported as unexpected, which it was: the
//! channels close before the server's `Leave` is processed, so the flag marking a terminal
//! disconnect is not set yet when the state change fires.
//!
//! The only observable is a log line, so this suite owns its own test binary and installs a
//! capturing logger -- a logger is global to a process, and every other suite leaves that to
//! `test_log`.
//!
//! Environment variables (same as the other e2e suites):
//! - LIVEKIT_URL (default ws://localhost:7880)
//! - LIVEKIT_API_KEY (default "devkey")
//! - LIVEKIT_API_SECRET (default "secret")
//!
//! Run:
//!   cargo test -p livekit --features "__lk-e2e-test,native-tls" --test data_channel_close_log_test -- --nocapture

#![cfg(feature = "__lk-e2e-test")]

#[cfg(feature = "__lk-e2e-test")]
use {
    anyhow::{anyhow, bail, Result},
    libwebrtc::{
        native::create_random_uuid,
        prelude::{RtcVideoSource, VideoResolution},
        video_source::native::NativeVideoSource,
    },
    livekit::{
        options::TrackPublishOptions,
        track::{LocalTrack, LocalVideoTrack},
        ConnectionState, DisconnectReason, Room, RoomEvent, RoomOptions,
    },
    livekit_api::services::room::RoomClient,
    livekit_token::{AccessToken, VideoGrants},
    std::{
        env,
        sync::{Mutex, Once},
        time::Duration,
    },
    tokio::time::{self, timeout},
};

/// Longer than `rtc_session::DATA_CHANNEL_CLOSE_GRACE_PERIOD`, so a report the fix is meant to
/// suppress has already been emitted by the time the assertion runs. The constant is private.
const PAST_GRACE_PERIOD: Duration = Duration::from_secs(3);

static RECORDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static INSTALL: Once = Once::new();

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let line = format!("{} {} - {}", record.level(), record.target(), record.args());
        eprintln!("{line}"); // still visible under --nocapture
        RECORDS.lock().unwrap().push(line);
    }

    fn flush(&self) {}
}

fn capture_logs() {
    INSTALL.call_once(|| {
        log::set_logger(&Capture).expect("the capturing logger must own this test binary");
        log::set_max_level(log::LevelFilter::Debug);
    });
}

fn logged(needle: &str) -> Vec<String> {
    RECORDS.lock().unwrap().iter().filter(|line| line.contains(needle)).cloned().collect()
}

/// A server-side room deletion closes the publisher data channels from the remote side. The
/// close belongs to an expected teardown, so nothing may report it as unexpected.
#[tokio::test(flavor = "multi_thread")]
async fn test_server_teardown_does_not_report_unexpected_data_channel_close() -> Result<()> {
    capture_logs();

    let api_key = env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| "devkey".into());
    let api_secret = env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| "secret".into());
    let server_url = env::var("LIVEKIT_URL").unwrap_or_else(|_| "ws://localhost:7880".into());

    let room_name = format!("test_room_{}", create_random_uuid());
    let token = AccessToken::with_api_key(&api_key, &api_secret)
        .with_ttl(Duration::from_secs(30 * 60))
        .with_grants(VideoGrants { room_join: true, room: room_name.clone(), ..Default::default() })
        .with_identity("p0")
        .with_name("Participant 0")
        .to_jwt()?;

    let (room, mut events) = Room::connect(&server_url, &token, RoomOptions::default()).await?;
    assert_eq!(room.connection_state(), ConnectionState::Connected);

    // Publishing brings the publisher peer connection up, which is what makes the close look
    // unexpected at all: a close is only reported while the publisher is connected.
    let source = NativeVideoSource::new(VideoResolution { width: 16, height: 16 }, false);
    let track = LocalVideoTrack::create_video_track(
        "teardown-track",
        RtcVideoSource::Native(source.clone()),
    );
    let publication = room
        .local_participant()
        .publish_track(LocalTrack::Video(track), TrackPublishOptions::default())
        .await?;
    assert!(publication.track().is_some());

    let http_url = server_url.replacen("ws", "http", 1);
    RoomClient::with_api_key(&http_url, &api_key, &api_secret).delete_room(&room_name).await?;

    let observe = async {
        while let Some(event) = events.recv().await {
            if let RoomEvent::Disconnected { reason } = event {
                return Ok(reason);
            }
        }
        bail!("event stream ended before the room reported Disconnected");
    };
    let reason = timeout(Duration::from_secs(30), observe).await??;
    assert_eq!(reason, DisconnectReason::RoomDeleted);

    time::sleep(PAST_GRACE_PERIOD).await;

    let reported = logged("closed unexpectedly");
    if !reported.is_empty() {
        return Err(anyhow!(
            "an expected teardown reported {} unexpected data channel close(s):\n  {}",
            reported.len(),
            reported.join("\n  ")
        ));
    }

    // The close still has to be seen -- an assertion that passes because no channel ever closed
    // would not be guarding anything.
    assert!(
        !logged("closed during teardown").is_empty(),
        "no publisher data channel closed, so this run proves nothing about how closes are reported"
    );

    drop(publication);
    drop(source);
    drop(events);
    drop(room);
    Ok(())
}
