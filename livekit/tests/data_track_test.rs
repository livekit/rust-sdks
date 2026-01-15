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
    common::test_rooms,
    futures_util::StreamExt,
    livekit::{data_track::DataTrackOptions, RoomEvent},
    std::time::Duration,
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
        let track = pub_room
            .local_participant()
            .publish_data_track(DataTrackOptions::with_name("my_track"))
            .await?;
        log::info!("Track published");

        assert!(track.is_published());
        assert!(!track.info().uses_e2ee());
        assert_eq!(track.info().name(), "my_track");

        let sleep_duration = Duration::from_secs_f64(1.0 / publish_fps as f64);
        for index in 0..frame_count {
            track.publish(vec![index as u8; payload_len].into())?;
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
