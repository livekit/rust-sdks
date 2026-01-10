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
    anyhow::{Ok, Result},
    common::test_rooms,
    futures_util::StreamExt,
    livekit::{data_track::DataTrackOptions, RoomEvent},
    std::time::Duration,
    tokio::{time::timeout, try_join},
};

mod common;

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_data_track() -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (pub_room, _) = rooms.pop().unwrap();
    let (_, mut sub_room_event_rx) = rooms.pop().unwrap();
    let pub_identity = pub_room.local_participant().identity();

    const FRAME_COUNT: usize = 16;
    const FRAME_PAYLOAD: &[u8] = &[0xFA; 256];

    let publish_track = async move {
        let track = pub_room
            .local_participant()
            .publish_data_track(DataTrackOptions::with_name("my_track"))
            .await?;

        assert!(track.is_published());
        assert!(track.info().sid().starts_with("DTR_"));
        assert!(!track.info().uses_e2ee());
        assert_eq!(track.info().name(), "my_track");

        for _ in 0..FRAME_COUNT {
            track.publish(FRAME_PAYLOAD.into())?;
        }
        Ok(())
    };

    let subscribe_to_track = async move {
        while let Some(event) = sub_room_event_rx.recv().await {
            let RoomEvent::RemoteDataTrackPublished(track) = event else {
                continue;
            };
            assert!(track.is_published());
            assert!(track.info().sid().starts_with("DTR_"));
            assert!(!track.info().uses_e2ee());
            assert_eq!(track.info().name(), "my_track");
            assert_eq!(track.publisher_identity(), pub_identity.as_str());

            let mut frame_stream = track.subscribe().await?;
            while let Some(frame) = frame_stream.next().await {
                assert_eq!(frame.payload(), FRAME_PAYLOAD);
                assert_eq!(frame.user_timestamp(), None);
            }
            break;
        }
        Ok(())
    };
    timeout(Duration::from_secs(5), async { try_join!(publish_track, subscribe_to_track) })
        .await??;
    Ok(())
}
