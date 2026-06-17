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
    livekit::{prelude::*, SimulateScenario},
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
#[test_case(8_192 ; "single_packet")]
#[test_case(196_608 ; "multi_packet")]
#[test_log::test(tokio::test)]
async fn test_data_track(payload_len: usize) {
    let mut rooms = test_rooms(2).await.unwrap();

    let (pub_room, _) = rooms.pop().unwrap();
    let (_, mut sub_room_event_rx) = rooms.pop().unwrap();
    let pub_identity = pub_room.local_participant().identity();

    let local_track = pub_room.local_participant().publish_data_track("my_track").await.unwrap();
    log::info!("Track published");

    let remote_track = wait_for_remote_track(&mut sub_room_event_rx).await.unwrap();
    log::info!("Got remote track: {}", remote_track.info().sid());

    const PAYLOAD_VALUE: u8 = 0xFA;

    let publish = async move {
        assert!(local_track.is_published());
        assert!(!local_track.info().uses_e2ee());
        assert_eq!(local_track.info().name(), "my_track");

        let payload = vec![PAYLOAD_VALUE; payload_len];
        loop {
            local_track.try_push(payload.clone().into()).unwrap();
            time::sleep(Duration::from_millis(50)).await;
        }
    };

    let subscribe = async move {
        assert!(remote_track.is_published());
        assert!(!remote_track.info().uses_e2ee());
        assert_eq!(remote_track.info().name(), "my_track");
        assert_eq!(remote_track.publisher_identity(), pub_identity.as_str());

        let mut subscription = remote_track.subscribe().await.unwrap();

        let mut got_frame = false;
        while let Some(frame) = subscription.next().await {
            let payload = frame.payload();
            assert_eq!(payload.len(), payload_len);

            assert!(payload.iter().all(|byte| *byte == PAYLOAD_VALUE));
            assert_eq!(frame.user_timestamp(), None);

            got_frame = true;
            break;
        }
        assert!(got_frame, "No frame received");
        assert!(remote_track.is_published());
    };

    timeout(Duration::from_secs(15), async {
        tokio::select! { _ = publish => (), _ = subscribe => () };
    })
    .await
    .expect("Timed out waiting for frame");
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
#[test_log::test(tokio::test)]
async fn test_publish_with_schema_and_frame_encoding() -> Result<()> {
    use livekit::data_track::{DataTrackFrameEncoding, DataTrackSchemaEncoding, DataTrackSchemaId};

    let mut rooms = test_rooms(2).await?;
    let (pub_room, _) = rooms.pop().unwrap();
    let (_, mut sub_room_event_rx) = rooms.pop().unwrap();

    let schema_id = DataTrackSchemaId::new("my_schema", DataTrackSchemaEncoding::JsonSchema);
    let frame_encoding = DataTrackFrameEncoding::Json;

    let options = DataTrackOptions::new("my_track")
        .with_schema(schema_id.clone())
        .with_frame_encoding(frame_encoding);

    let local_track = pub_room.local_participant().publish_data_track(options).await?;
    assert_eq!(local_track.info().schema(), Some(&schema_id));
    assert_eq!(local_track.info().frame_encoding(), Some(frame_encoding));

    // The subscriber should observe the same schema and frame encoding metadata.
    let remote_track =
        timeout(Duration::from_secs(5), wait_for_remote_track(&mut sub_room_event_rx)).await??;
    assert_eq!(remote_track.info().schema(), Some(&schema_id));
    assert_eq!(remote_track.info().frame_encoding(), Some(frame_encoding));

    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_e2ee() -> Result<()> {
    use livekit::e2ee::{
        key_provider::{KeyProvider, KeyProviderOptions},
        EncryptionType,
    };
    use livekit::E2eeOptions;

    const SHARED_SECRET: &[u8] = b"password";
    const PAYLOAD: &[u8] = &[0xFA; 196_608];

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
        let track = pub_room.local_participant().publish_data_track("my_track").await.unwrap();
        assert!(track.info().uses_e2ee());
        loop {
            _ = track.try_push(PAYLOAD.into());
            time::sleep(Duration::from_millis(125)).await;
        }
    };

    let subscribe = async move {
        let track = wait_for_remote_track(&mut sub_room_event_rx).await.unwrap();

        assert!(track.info().uses_e2ee());
        let mut subscription = track.subscribe().await.unwrap();

        let mut got_frame = false;
        while let Some(frame) = subscription.next().await {
            assert_eq!(frame.payload(), PAYLOAD);
            got_frame = true;
            break;
        }
        assert!(got_frame);
    };

    let _ = timeout(Duration::from_secs(5), async {
        tokio::select! { _ = publish => (), _ = subscribe => () };
    })
    .await?;

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
        let track = wait_for_remote_track(&mut sub_room_event_rx).await?;
        assert!(track.is_published());

        let elapsed = {
            let start = Instant::now();
            track.wait_for_unpublish().await;
            start.elapsed()
        };
        assert!(elapsed.abs_diff(PUBLISH_DURATION) <= Duration::from_millis(20));
        assert!(!track.is_published());

        Ok(())
    };

    timeout(Duration::from_secs(5), async { try_join!(publish, subscribe) }).await??;
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_resubscribe() -> Result<()> {
    const ITERATIONS: usize = 10;

    let mut rooms = test_rooms(2).await?;

    let (pub_room, _) = rooms.pop().unwrap();
    let (_, mut sub_room_event_rx) = rooms.pop().unwrap();

    let publish = async move {
        let track = pub_room.local_participant().publish_data_track("my_track").await.unwrap();
        loop {
            _ = track.try_push(vec![0xFA; 64].into());
            time::sleep(Duration::from_millis(50)).await;
        }
    };

    let subscribe = async move {
        let track = wait_for_remote_track(&mut sub_room_event_rx).await.unwrap();

        let mut successful_subscriptions = 0;
        for _ in 0..ITERATIONS {
            let mut stream = track.subscribe().await.unwrap();
            while let Some(frame) = stream.next().await {
                // Ensure we can at least get one frame.
                assert!(!frame.payload().is_empty());
                successful_subscriptions += 1;
                break;
            }
            std::mem::drop(stream);
            time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(successful_subscriptions, ITERATIONS);
    };

    let _ = timeout(Duration::from_secs(5), async {
        tokio::select! { _ = publish => (), _ = subscribe => () };
    })
    .await?;
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_frame_with_user_timestamp() -> Result<()> {
    let mut rooms = test_rooms(2).await?;

    let (pub_room, _) = rooms.pop().unwrap();
    let (_, mut sub_room_event_rx) = rooms.pop().unwrap();

    let publish = async move {
        let track = pub_room.local_participant().publish_data_track("my_track").await.unwrap();
        loop {
            let frame = DataTrackFrame::new(vec![0xFA; 64]).with_user_timestamp_now();
            _ = track.try_push(frame);
            time::sleep(Duration::from_millis(50)).await;
        }
    };

    let subscribe = async move {
        let track = wait_for_remote_track(&mut sub_room_event_rx).await.unwrap();

        let mut stream = track.subscribe().await.unwrap();
        let mut got_frame = false;
        while let Some(frame) = stream.next().await {
            // Ensure we can at least get one frame.
            assert!(!frame.payload().is_empty());
            let duration = frame.duration_since_timestamp().expect("Missing timestamp");
            assert!(duration.as_millis() < 1000);
            got_frame = true;
            break;
        }
        if !got_frame {
            panic!("No frame received");
        }
    };

    let _ = timeout(Duration::from_secs(5), async {
        tokio::select! { _ = publish => (), _ = subscribe => () };
    })
    .await?;
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_case(SimulateScenario::SignalReconnect; "signal_reconnect")]
#[test_case(SimulateScenario::ForceTcp; "full_reconnect")]
#[test_log::test(tokio::test)]
async fn test_subscriber_side_fault(scenario: SimulateScenario) -> Result<()> {
    let mut rooms = test_rooms(2).await?;

    let (pub_room, _) = rooms.pop().unwrap();
    let (sub_room, mut sub_room_event_rx) = rooms.pop().unwrap();

    let publish = async move {
        let track = pub_room.local_participant().publish_data_track("my_track").await.unwrap();
        loop {
            _ = track.try_push(vec![0xFA; 64].into());
            time::sleep(Duration::from_millis(50)).await;
        }
    };

    let subscribe = async move {
        let track = wait_for_remote_track(&mut sub_room_event_rx).await.unwrap();
        let mut stream = track.subscribe().await.unwrap();

        // TODO: this should also evaluate what happens if a track subscription is removed
        // during a full reconnect event.
        sub_room.simulate_scenario(scenario).await.unwrap();
        assert!(track.is_published());

        let mut got_frame = false;
        while let Some(frame) = stream.next().await {
            // Ensure we can at least get one frame.
            assert!(!frame.payload().is_empty());
            got_frame = true;
            break;
        }
        if !got_frame {
            panic!("No frame received");
        }
    };

    let _ = timeout(Duration::from_secs(15), async {
        tokio::select! { _ = publish => (), _ = subscribe => () };
    })
    .await?;
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_case(SimulateScenario::SignalReconnect; "signal_reconnect")]
#[test_case(SimulateScenario::ForceTcp; "full_reconnect")]
#[test_log::test(tokio::test)]
async fn test_publisher_side_fault(scenario: SimulateScenario) -> Result<()> {
    let mut rooms = test_rooms(1).await?;
    let (pub_room, _) = rooms.pop().unwrap();

    let publish = async move {
        let track = pub_room.local_participant().publish_data_track("my_track").await.unwrap();
        let initial_sid = track.info().sid().clone();

        pub_room.simulate_scenario(scenario).await.unwrap();
        assert!(track.is_published(), "Should still be reported as published");

        if scenario == SimulateScenario::ForceTcp {
            // Republish (full reconnect → new RtcSession → new sid) is async.
            // Poll up to 8s for the new sid instead of unconditionally sleeping
            // a fixed window:
            let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
            loop {
                if track.info().sid() != initial_sid {
                    break;
                }
                if tokio::time::Instant::now() >= deadline {
                    panic!(
                        "Should have new SID after ForceTcp reconnect (still {:?})",
                        initial_sid
                    );
                }
                time::sleep(Duration::from_millis(100)).await;
            }
        }

        assert!(track.is_published(), "Should still be reported as published");
        track.try_push(vec![0xFA; 64].into()).expect("Should be able to push frame");
    };

    let _ = timeout(Duration::from_secs(15), publish).await?;
    Ok(())
}

/// Waits for the first remote data track to be published.
#[cfg(feature = "__lk-e2e-test")]
async fn wait_for_remote_track(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
) -> Result<RemoteDataTrack> {
    while let Some(event) = rx.recv().await {
        if let RoomEvent::DataTrackPublished(track) = event {
            return Ok(track);
        }
    }
    Err(anyhow!("No track published"))
}
