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
    common::test_rooms_with_options,
    futures_util::StreamExt,
    livekit::{data_track::DataTrackOptions, RoomEvent, RoomOptions},
    std::{iter, time::Duration},
    tokio::time::{self, timeout},
};

mod common;

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_data_track() -> Result<()> {
    // Temporary workaround until auto subscribe is disabled on the SFU.
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = false;

    let mut rooms = test_rooms_with_options(iter::repeat(room_options.clone()).take(2)).await?;

    // let mut rooms = test_rooms(2).await?;

    let (pub_room, _) = rooms.pop().unwrap();
    let (_, mut sub_room_event_rx) = rooms.pop().unwrap();
    let pub_identity = pub_room.local_participant().identity();

    // How many frames to check on the subscriber side.
    const FRAME_VERIFY_COUNT: usize = 16;
    const FRAME_PAYLOAD: &[u8] = &[0xFA; 256];

    let publish = async move {
        let track = pub_room
            .local_participant()
            .publish_data_track(DataTrackOptions::with_name("my_track"))
            .await?;
        log::info!("Track published: {:?}", track);

        assert!(track.is_published());
        assert!(track.info().sid().starts_with("DTR_"));
        assert!(!track.info().uses_e2ee());
        assert_eq!(track.info().name(), "my_track");

        while track.is_published() {
            track.publish(FRAME_PAYLOAD.into())?;
            time::sleep(Duration::from_millis(25)).await;
        }
        Ok(())
    };

    let subscribe_until_verified = async move {
        while let Some(event) = sub_room_event_rx.recv().await {
            let RoomEvent::RemoteDataTrackPublished(track) = event else {
                continue;
            };
            log::info!("Got remote track: {:?}", track);

            assert!(track.is_published());
            assert!(track.info().sid().starts_with("DTR_"));
            assert!(!track.info().uses_e2ee());
            assert_eq!(track.info().name(), "my_track");
            assert_eq!(track.publisher_identity(), pub_identity.as_str());

            let mut frame_count = 0;
            let mut frame_stream = track.subscribe().await?;

            while let Some(frame) = frame_stream.next().await {
                assert_eq!(frame.payload(), FRAME_PAYLOAD);
                assert_eq!(frame.user_timestamp(), None);

                frame_count += 1;
                if frame_count >= FRAME_VERIFY_COUNT {
                    break;
                };
            }
            break;
        }
        Ok(())
    };

    let verify_frames_received = async {
        tokio::select! {
            res = publish => res?,
            res = subscribe_until_verified => res?
        }
        Ok(())
    };
    timeout(Duration::from_secs(10), verify_frames_received).await??;
    Ok(())
}
