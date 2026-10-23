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

#[cfg(feature = "__lk-e2e-test")]
use {
    anyhow::{Ok, Result},
    common::test_rooms,
    futures_util::future::try_join_all,
    libwebrtc::{
        audio_source::native::NativeAudioSource,
        prelude::{AudioSourceOptions, RtcAudioSource},
    },
    livekit::{options::TrackPublishOptions, prelude::*},
    std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    },
};

mod common;

/// Counts the ERROR records the publisher negotiation logs when it fails, and prints any other
/// ERROR record. It is the global logger of this test binary, so the binary holds one test.
#[cfg(feature = "__lk-e2e-test")]
struct FailedNegotiationCounter;

#[cfg(feature = "__lk-e2e-test")]
static FAILED_NEGOTIATIONS: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "__lk-e2e-test")]
impl log::Log for FailedNegotiationCounter {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Error
    }

    fn log(&self, record: &log::Record) {
        if record.level() != log::Level::Error {
            return;
        }
        let message = record.args().to_string();
        if message.contains("failed to negotiate the publisher") {
            FAILED_NEGOTIATIONS.fetch_add(1, Ordering::Relaxed);
        } else {
            eprintln!("ERROR {}: {}", record.target(), message);
        }
    }

    fn flush(&self) {}
}

#[cfg(feature = "__lk-e2e-test")]
async fn publish_and_close() -> Result<()> {
    let mut rooms = test_rooms(1).await?;
    let (room, _events) = rooms.pop().unwrap();
    for name in ["a", "b"] {
        let source = NativeAudioSource::new(AudioSourceOptions::default(), 48_000, 1, 100);
        let track = LocalAudioTrack::create_audio_track(name, RtcAudioSource::Native(source));
        room.local_participant()
            .publish_track(LocalTrack::Audio(track), TrackPublishOptions::default())
            .await?;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    room.close().await?;
    Ok(())
}

/// Closing a room must not log a failed publisher negotiation.
///
/// `Room::close` unpublishes the local tracks before it closes the engine. A renegotiation
/// started by those unpublishes, or one still running for the last publish, can race the close of
/// the publisher peer connection and log "failed to negotiate the publisher" when the
/// connection closes under it. Several rooms close at once on a multi-threaded runtime to give
/// the race a chance to show.
#[cfg(feature = "__lk-e2e-test")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_close_logs_no_publisher_negotiation_error() -> Result<()> {
    const ROUNDS: usize = 5;
    const ROOMS_PER_ROUND: usize = 6;

    log::set_logger(&FailedNegotiationCounter).unwrap();
    log::set_max_level(log::LevelFilter::Error);

    for _ in 0..ROUNDS {
        try_join_all((0..ROOMS_PER_ROUND).map(|_| publish_and_close())).await?;
        // Lets negotiations that outlive `close()` log before the next round, and the count.
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    let failed = FAILED_NEGOTIATIONS.load(Ordering::Relaxed);
    assert_eq!(
        failed,
        0,
        "{failed} failed publisher negotiations logged over {} room closes",
        ROUNDS * ROOMS_PER_ROUND
    );
    Ok(())
}
