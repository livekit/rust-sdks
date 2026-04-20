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
    anyhow::{anyhow, Result},
    common::{
        test_rooms_with_options,
        video::{SolidColorParams, SolidColorTrack},
        TestRoomOptions,
    },
    livekit::{options::VideoCodec, prelude::*, track::VideoQuality},
    std::{sync::Arc, time::Duration},
    tokio::time::{self, timeout},
};

mod common;

/// Extracts the `LocalVideoTrack` from the publisher's first video track publication.
#[cfg(feature = "__lk-e2e-test")]
fn publisher_video_track(room: &Room) -> Result<LocalVideoTrack> {
    for pub_ in room.local_participant().track_publications().values() {
        if let Some(LocalTrack::Video(vt)) = pub_.track() {
            return Ok(vt);
        }
    }
    Err(anyhow!("No local video track publication found"))
}

/// Polls `publishing_layers()` until the `check` predicate returns true, or times out.
#[cfg(feature = "__lk-e2e-test")]
async fn wait_for_layers(
    track: &LocalVideoTrack,
    label: &str,
    max_wait: Duration,
    check: impl Fn(&[(String, String, bool)]) -> bool,
) -> Result<Vec<(String, String, bool)>> {
    let deadline = tokio::time::Instant::now() + max_wait;
    loop {
        let layers = track.publishing_layers();
        log::info!("dynacast test [{}]: layers = {:?}", label, layers);
        if check(&layers) {
            return Ok(layers);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!(
                "dynacast test [{}]: timed out waiting for expected layer state, last = {:?}",
                label,
                layers
            ));
        }
        time::sleep(Duration::from_millis(250)).await;
    }
}

/// Verifies that dynacast toggles publisher simulcast layers in response to subscriber quality
/// requests.
///
/// 1. Publisher connects with `dynacast: true` and publishes a simulcast VP8 track.
/// 2. Subscriber receives the track -- baseline expects all layers active.
/// 3. Subscriber requests LOW quality via `set_video_quality` -- the SFU should send a
///    `SubscribedQualityUpdate` that disables the higher layers.
/// 4. Subscriber requests HIGH quality again -- all layers should re-activate.
///
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_dynacast() -> Result<()> {
    let mut pub_room_opts = RoomOptions::default();
    pub_room_opts.dynacast = true;
    let pub_options = TestRoomOptions { room: pub_room_opts, ..Default::default() };
    let sub_options = TestRoomOptions::default();

    let mut rooms = test_rooms_with_options([pub_options, sub_options]).await?;
    let (pub_room, _pub_events) = rooms.remove(0);
    let (_sub_room, mut sub_events) = rooms.remove(0);

    let pub_room = Arc::new(pub_room);
    let solid_params = SolidColorParams { width: 1280, height: 720, luma: 128 };
    let mut solid_track = SolidColorTrack::new(pub_room.clone(), solid_params);
    solid_track.publish(VideoCodec::VP8, true).await?;

    let sub_publication: RemoteTrackPublication = timeout(Duration::from_secs(15), async {
        loop {
            let Some(event) = sub_events.recv().await else {
                return Err(anyhow!("Event channel closed before TrackSubscribed"));
            };
            if let RoomEvent::TrackSubscribed { publication, .. } = event {
                return Ok(publication);
            }
        }
    })
    .await??;

    let pub_video_track = publisher_video_track(&pub_room)?;

    // --- Baseline: all simulcast layers should be active after initial subscription ---
    let layers = wait_for_layers(
        &pub_video_track,
        "baseline",
        Duration::from_secs(15),
        |layers| layers.len() > 1 && layers.iter().all(|(_, _, active)| *active),
    )
    .await?;
    log::info!("dynacast baseline layers: {:?}", layers);
    assert!(layers.len() > 1, "expected multiple simulcast layers, got {}", layers.len());

    // --- Request LOW quality: SFU should tell publisher to deactivate Medium and High ---
    log::info!("dynacast test: requesting LOW quality");
    sub_publication.set_video_quality(VideoQuality::Low);

    let layers = wait_for_layers(
        &pub_video_track,
        "after LOW request",
        Duration::from_secs(30),
        |layers| {
            let low_active = layers.iter().any(|(_, q, a)| q == "Low" && *a);
            let high_inactive = layers.iter().filter(|(_, q, _)| q != "Low").all(|(_, _, a)| !*a);
            low_active && high_inactive
        },
    )
    .await?;
    log::info!("dynacast layers after LOW request: {:?}", layers);
    assert!(
        layers.iter().any(|(_, q, a)| q == "Low" && *a),
        "expected Low layer to be active, got {:?}",
        layers
    );
    assert!(
        layers.iter().filter(|(_, q, _)| q != "Low").all(|(_, _, a)| !*a),
        "expected Medium and High layers to be inactive, got {:?}",
        layers
    );

    // --- Request HIGH quality: all layers should become active again ---
    log::info!("dynacast test: requesting HIGH quality");
    sub_publication.set_video_quality(VideoQuality::High);

    let layers = wait_for_layers(
        &pub_video_track,
        "after HIGH request",
        Duration::from_secs(30),
        |layers| layers.len() > 1 && layers.iter().all(|(_, _, active)| *active),
    )
    .await?;
    log::info!("dynacast layers after HIGH request: {:?}", layers);
    assert!(
        layers.iter().all(|(_, _, active)| *active),
        "expected all layers active after HIGH request, got {:?}",
        layers
    );

    Ok(())
}
