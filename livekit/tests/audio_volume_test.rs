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
    anyhow::{anyhow, Ok, Result},
    common::{
        audio::{SineParameters, SineTrack},
        test_rooms,
    },
    libwebrtc::stats::RtcStats,
    livekit::prelude::*,
    std::{sync::Arc, time::Duration},
    tokio::{sync::mpsc::UnboundedReceiver, time::timeout},
};

mod common;

#[cfg(feature = "__lk-e2e-test")]
const SINE: SineParameters =
    SineParameters { sample_rate: 48_000, freq: 440.0, amplitude: 0.25, num_channels: 1 };

/// Waits for the first remote audio track the subscriber is given.
#[cfg(feature = "__lk-e2e-test")]
async fn subscribed_audio(events: &mut UnboundedReceiver<RoomEvent>) -> Result<RemoteAudioTrack> {
    loop {
        let event = events.recv().await.ok_or_else(|| anyhow!("Never received track"))?;
        if let RoomEvent::TrackSubscribed { track: RemoteTrack::Audio(track), .. } = event {
            return Ok(track);
        }
    }
}

/// The subscriber's playout level, which WebRTC measures after the volume is applied.
#[cfg(feature = "__lk-e2e-test")]
async fn playout_level(track: &RemoteAudioTrack) -> Result<f64> {
    track
        .get_stats()
        .await?
        .into_iter()
        .find_map(|stats| match stats {
            RtcStats::InboundRtp(inbound) => Some(inbound.inbound.audio_level),
            _ => None,
        })
        .ok_or_else(|| anyhow!("No inbound RTP stats"))
}

/// Polls the playout level until `accept` holds, returning the level that satisfied it.
#[cfg(feature = "__lk-e2e-test")]
async fn level_until(track: &RemoteAudioTrack, accept: impl Fn(f64) -> bool) -> Result<f64> {
    timeout(Duration::from_secs(10), async {
        loop {
            let level = playout_level(track).await?;
            if accept(level) {
                return Ok(level);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .map_err(|_| anyhow!("Playout level never settled"))?
}

/// Tests that a remote track's volume changes what is played out on this device.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_remote_audio_volume() -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (pub_room, _) = rooms.pop().unwrap();
    let (_sub_room, mut sub_events) = rooms.pop().unwrap();
    let mut sine = SineTrack::new(Arc::new(pub_room), SINE);
    sine.publish().await?;
    let track = timeout(Duration::from_secs(10), subscribed_audio(&mut sub_events)).await??;

    let unchanged = level_until(&track, |level| level > 0.05).await?;
    track.set_volume(0.0);
    level_until(&track, |level| level < 0.01).await?;
    track.set_volume(1.0);
    level_until(&track, |level| level > unchanged * 0.7).await?;
    track.set_volume(2.0);
    level_until(&track, |level| level > unchanged * 1.5).await?;

    // Out of WebRTC's range: clamped instead of tripping its checks.
    track.set_volume(-1.0);
    level_until(&track, |level| level < 0.01).await?;
    track.set_volume(f64::NAN);
    level_until(&track, |level| level > unchanged * 0.7).await?;
    track.set_volume(f64::INFINITY);
    level_until(&track, |level| level > unchanged * 1.5).await?;

    sine.unpublish().await?;
    Ok(())
}

/// Repeated initialize → use → disconnect/drop → shutdown workload for the volume path: changes
/// come from threads outside the async runtime while the track is live, and keep coming once the
/// subscriber's room is closed and the track is gone from it.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_remote_audio_volume_lifecycle() -> Result<()> {
    for _ in 0..3 {
        let mut rooms = test_rooms(2).await?;
        let (pub_room, _) = rooms.pop().unwrap();
        let (sub_room, mut sub_events) = rooms.pop().unwrap();
        let mut sine = SineTrack::new(Arc::new(pub_room), SINE);
        sine.publish().await?;
        let track = timeout(Duration::from_secs(10), subscribed_audio(&mut sub_events)).await??;

        let threads: Vec<_> = (0..4)
            .map(|at| {
                let track = track.clone();
                std::thread::spawn(move || {
                    for step in 0..50 {
                        track.set_volume(f64::from((at + step) % 12) - 1.0);
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().map_err(|_| anyhow!("A volume thread panicked"))?;
        }

        sine.unpublish().await?;
        sub_room.close().await?;
        let room_dropped = sub_room.drop_probe();
        let track_dropped = track.drop_probe();
        drop(sub_room);
        drop(sub_events);
        // The track outlives its room here: changing it must still be harmless.
        track.set_volume(0.5);
        drop(track);

        // Teardown unwinds across tasks and the signaling thread, so poll for release.
        let released = timeout(Duration::from_secs(10), async {
            while !(room_dropped() && track_dropped()) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok();
        assert!(released, "subscriber room or remote audio track retained after drop");
    }
    Ok(())
}
