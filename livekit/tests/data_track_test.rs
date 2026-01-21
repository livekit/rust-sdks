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

#[cfg(feature = "__lk-e2e-test")]
use {
    anyhow::{anyhow, Ok, Result},
    common::{test_rooms, test_rooms_with_options, TestRoomOptions},
    futures_util::StreamExt,
    livekit::prelude::*,
    livekit_api::access_token::VideoGrants,
    std::time::{Duration, Instant},
    test_case::test_case,
    tokio::{
        time::{self, timeout},
        try_join,
    },
};

mod common;

#[cfg(feature = "__lk-e2e-test")]
#[test_case(120., 8_192 ; "high_fps_single_packet")]
#[test_case(10., 196_608 ; "low_fps_multi_packet")]
#[test_log::test(tokio::test)]
async fn test_data_track(publish_fps: f64, payload_len: usize) -> Result<()> {
    // How long to publish frames for.
    const PUBLISH_DURATION: Duration = Duration::from_secs(5);

    // Percentage of total frames that must be received on the subscriber end in
    // order for the test to pass.
    const MIN_PERCENTAGE: f32 = 0.95;

    let mut rooms = test_rooms(2).await?;

    let (pub_room, _) = rooms.pop().unwrap();
    let (_, mut sub_room_event_rx) = rooms.pop().unwrap();
    let pub_identity = pub_room.local_participant().identity();

    let frame_count = (PUBLISH_DURATION.as_secs_f64() * publish_fps).round() as u64;
    log::info!("Publishing {} frames", frame_count);

    let publish = async move {
        let track = pub_room.local_participant().publish_data_track("my_track").await?;
        log::info!("Track published");

        assert!(track.is_published());
        assert!(!track.info().uses_e2ee());
        assert_eq!(track.info().name(), "my_track");

        let sleep_duration = Duration::from_secs_f64(1.0 / publish_fps as f64);
        for index in 0..frame_count {
            track.try_push(vec![index as u8; payload_len].into())?;
            time::sleep(sleep_duration).await;
        }
        Ok(())
    };

    let subscribe = async move {
        let track = async move {
            while let Some(event) = sub_room_event_rx.recv().await {
                let RoomEvent::RemoteDataTrackPublished(track) = event else {
                    continue;
                };
                return Ok(track);
            }
            Err(anyhow!("No track published"))
        }
        .await?;

        log::info!("Got remote track: {}", track.info().sid());
        assert!(track.is_published());
        assert!(!track.info().uses_e2ee());
        assert_eq!(track.info().name(), "my_track");
        assert_eq!(track.publisher_identity(), pub_identity.as_str());

        let mut subscription = track.subscribe().await?;

        let mut recv_count = 0;

        while let Some(frame) = subscription.next().await {
            let payload = frame.payload();
            if let Some(first_byte) = payload.first() {
                assert!(payload.iter().all(|byte| byte == first_byte));
            }
            assert_eq!(frame.user_timestamp(), None);
            recv_count += 1;
        }

        let recv_percent = recv_count as f32 / frame_count as f32;
        log::info!("Received {}/{} frames ({:.2}%)", recv_count, frame_count, recv_percent * 100.);

        if recv_percent < MIN_PERCENTAGE {
            Err(anyhow!("Not enough frames received"))?;
        }
        Ok(())
    };
    timeout(PUBLISH_DURATION + Duration::from_secs(5), async { try_join!(publish, subscribe) })
        .await??;
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_publish_many_tracks() -> Result<()> {
    const TRACK_COUNT: usize = 256;

    let (room, _) = test_rooms(1).await?.pop().unwrap();

    let publish_tracks = async {
        let mut tracks = Vec::with_capacity(TRACK_COUNT);
        let start = Instant::now();

        for idx in 0..TRACK_COUNT {
            let name = format!("track_{}", idx);
            let track = room.local_participant().publish_data_track(name.clone()).await?;

            assert!(track.is_published());
            assert_eq!(track.info().name(), name);

            tracks.push(track);
        }

        let elapsed = start.elapsed();
        log::info!(
            "Publishing {} tracks took {:.2?} (average {:.2?} per track)",
            TRACK_COUNT,
            elapsed,
            elapsed / TRACK_COUNT as u32
        );
        Ok(tracks)
    };

    let tracks = timeout(Duration::from_secs(5), publish_tracks).await??;
    for track in &tracks {
        // Publish a single large frame per track.
        track.try_push(vec![0xFA; 196_608].into())?;
    }
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_publish_unauthorized() -> Result<()> {
    let (room, _) = test_rooms_with_options([TestRoomOptions {
        grants: VideoGrants { room_join: true, can_publish_data: false, ..Default::default() },
        ..Default::default()
    }])
    .await?
    .pop()
    .unwrap();

    let result = room.local_participant().publish_data_track("my_track").await;
    assert!(matches!(result, Err(PublishError::NotAllowed)));

    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_publish_duplicate_name() -> Result<()> {
    let (room, _) = test_rooms(1).await?.pop().unwrap();

    #[allow(unused)]
    let first = room.local_participant().publish_data_track("first").await?;

    let second_result = room.local_participant().publish_data_track("first").await;
    assert!(matches!(second_result, Err(PublishError::DuplicateName)));

    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_case(false; "use_room_settings")]
#[test_case(true; "disabled_on_track")]
#[test_log::test(tokio::test)]
async fn test_e2ee(disable_on_track: bool) -> Result<()> {
    use livekit::e2ee::{
        key_provider::{KeyProvider, KeyProviderOptions},
        EncryptionType,
    };
    use livekit::E2eeOptions;

    const SHARED_SECRET: &[u8] = b"password";

    let key_provider1 =
        KeyProvider::with_shared_key(KeyProviderOptions::default(), SHARED_SECRET.to_vec());

    let mut options1 = RoomOptions::default();
    options1.encryption =
        Some(E2eeOptions { key_provider: key_provider1, encryption_type: EncryptionType::Gcm });

    let key_provider2 =
        KeyProvider::with_shared_key(KeyProviderOptions::default(), SHARED_SECRET.to_vec());

    let mut options2 = RoomOptions::default();
    options2.encryption =
        Some(E2eeOptions { key_provider: key_provider2, encryption_type: EncryptionType::Gcm });

    let mut rooms = test_rooms_with_options([options1.into(), options2.into()]).await?;

    let (pub_room, _) = rooms.pop().unwrap();
    let (sub_room, mut sub_room_event_rx) = rooms.pop().unwrap();

    pub_room.e2ee_manager().set_enabled(true);
    sub_room.e2ee_manager().set_enabled(true);

    let publish = async move {
        let options = DataTrackOptions::new("my_track").disable_e2ee(disable_on_track);
        let track = pub_room.local_participant().publish_data_track(options).await?;
        assert!(track.info().uses_e2ee() || disable_on_track);

        for index in 0..5 {
            track.try_push(vec![index as u8; 196_608].into())?;
            time::sleep(Duration::from_millis(25)).await;
        }
        Ok(())
    };

    let subscribe = async move {
        let track = async move {
            while let Some(event) = sub_room_event_rx.recv().await {
                let RoomEvent::RemoteDataTrackPublished(track) = event else {
                    continue;
                };
                return Ok(track);
            }
            Err(anyhow!("No track published"))
        }
        .await?;

        assert!(track.info().uses_e2ee() || disable_on_track);
        let mut subscription = track.subscribe().await?;

        while let Some(frame) = subscription.next().await {
            let payload = frame.payload();
            if let Some(first_byte) = payload.first() {
                assert!(payload.iter().all(|byte| byte == first_byte));
            }
        }
        Ok(())
    };
    timeout(Duration::from_secs(5), async { try_join!(publish, subscribe) }).await??;
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_published_state() -> Result<()> {
    // How long to leave the track published.
    const PUBLISH_DURATION: Duration = Duration::from_millis(500);

    let mut rooms = test_rooms(2).await?;

    let (pub_room, _) = rooms.pop().unwrap();
    let (_, mut sub_room_event_rx) = rooms.pop().unwrap();

    let publish = async move {
        let track = pub_room.local_participant().publish_data_track("my_track").await?;

        assert!(track.is_published());
        time::sleep(PUBLISH_DURATION).await;
        track.unpublish();

        Ok(())
    };

    let subscribe = async move {
        let track = async move {
            while let Some(event) = sub_room_event_rx.recv().await {
                let RoomEvent::RemoteDataTrackPublished(track) = event else {
                    continue;
                };
                return Ok(track);
            }
            Err(anyhow!("No track published"))
        }
        .await?;
        assert!(track.is_published());

        let elapsed = {
            let start = Instant::now();
            track.wait_for_unpublish().await;
            start.elapsed()
        };
        assert!(elapsed.abs_diff(PUBLISH_DURATION) <= Duration::from_millis(20),);
        assert!(!track.is_published());

        Ok(())
    };

    timeout(Duration::from_secs(5), async { try_join!(publish, subscribe) }).await??;
    Ok(())
}
