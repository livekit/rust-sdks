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
    std::{collections::HashMap, sync::Arc, time::Duration},
    tokio::{
        sync::mpsc::UnboundedReceiver,
        time::{self, timeout},
    },
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

/// Returns the publisher's local video tracks keyed by track SID.
#[cfg(feature = "__lk-e2e-test")]
fn publisher_video_tracks(room: &Room) -> HashMap<TrackSid, LocalVideoTrack> {
    room.local_participant()
        .track_publications()
        .into_values()
        .filter_map(|publication| {
            let sid = publication.sid();
            let Some(LocalTrack::Video(track)) = publication.track() else {
                return None;
            };
            Some((sid, track))
        })
        .collect()
}

/// Polls `publishing_layers()` until the `check` predicate returns true, or times out.
#[cfg(feature = "__lk-e2e-test")]
async fn wait_for_layers(
    track: &LocalVideoTrack,
    label: &str,
    max_wait: Duration,
    check: impl Fn(&[PublishingLayer]) -> bool,
) -> Result<Vec<PublishingLayer>> {
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

/// Waits for the publisher to expose `expected_count` local video tracks.
#[cfg(feature = "__lk-e2e-test")]
async fn wait_for_publisher_video_tracks(
    room: &Room,
    expected_count: usize,
    label: &str,
    max_wait: Duration,
) -> Result<HashMap<TrackSid, LocalVideoTrack>> {
    let deadline = tokio::time::Instant::now() + max_wait;
    loop {
        let tracks = publisher_video_tracks(room);
        if tracks.len() == expected_count {
            return Ok(tracks);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!(
                "dynacast test [{}]: timed out waiting for {} publisher video tracks, got {}",
                label,
                expected_count,
                tracks.len()
            ));
        }
        time::sleep(Duration::from_millis(250)).await;
    }
}

/// Waits for a subscriber to observe all expected remote track publications.
#[cfg(feature = "__lk-e2e-test")]
async fn wait_for_remote_publications(
    room: &Room,
    track_sids: &[TrackSid],
    label: &str,
    max_wait: Duration,
) -> Result<HashMap<TrackSid, RemoteTrackPublication>> {
    let deadline = tokio::time::Instant::now() + max_wait;
    loop {
        let publications: HashMap<_, _> = room
            .remote_participants()
            .into_values()
            .flat_map(|participant| participant.track_publications())
            .filter(|(sid, _)| track_sids.contains(sid))
            .collect();

        if publications.len() == track_sids.len() {
            return Ok(publications);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!(
                "dynacast test [{}]: timed out waiting for remote publications, got {}/{}",
                label,
                publications.len(),
                track_sids.len()
            ));
        }
        time::sleep(Duration::from_millis(250)).await;
    }
}

/// Subscribes to exactly one of the provided publications and waits for it to attach a track.
#[cfg(feature = "__lk-e2e-test")]
async fn set_only_subscribed(
    publications: &HashMap<TrackSid, RemoteTrackPublication>,
    events: &mut UnboundedReceiver<RoomEvent>,
    active_sid: &TrackSid,
    label: &str,
) -> Result<()> {
    let Some(active_publication) = publications.get(active_sid) else {
        return Err(anyhow!("dynacast test [{}]: missing publication {}", label, active_sid));
    };

    for (sid, publication) in publications {
        publication.set_subscribed(sid == active_sid);
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if active_publication.track().is_some() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!(
                "dynacast test [{}]: timed out waiting to subscribe to {}",
                label,
                active_sid
            ));
        }

        match timeout(Duration::from_millis(250), events.recv()).await {
            Ok(Some(RoomEvent::TrackSubscriptionFailed { track_sid, error, .. }))
                if track_sid == *active_sid =>
            {
                log::warn!(
                    "dynacast test [{}]: subscription failed for {}: {:?}; retrying",
                    label,
                    active_sid,
                    error
                );
                active_publication.set_subscribed(false);
                active_publication.set_subscribed(true);
            }
            Ok(Some(_)) | Err(_) => {}
            Ok(None) => return Err(anyhow!("dynacast test [{}]: event channel closed", label)),
        }
    }
}

/// Waits until the requested tracks have active layers and all other tracks are fully inactive.
#[cfg(feature = "__lk-e2e-test")]
async fn wait_for_requested_tracks_only(
    tracks: &[(TrackSid, LocalVideoTrack)],
    active_sids: &[TrackSid],
    label: &str,
    max_wait: Duration,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + max_wait;
    loop {
        let mut states = Vec::with_capacity(tracks.len());
        let expected = tracks.iter().all(|(sid, track)| {
            let layers = track.publishing_layers();
            let active_count = layers.iter().filter(|layer| layer.active).count();
            let should_publish = active_sids.contains(sid);
            states.push(format!(
                "{}={}/{} active ({})",
                sid,
                active_count,
                layers.len(),
                if should_publish { "requested" } else { "not requested" }
            ));

            !layers.is_empty()
                && if should_publish {
                    layers.len() > 1 && layers.iter().all(|layer| layer.active)
                } else {
                    layers.iter().all(|layer| !layer.active)
                }
        });

        log::info!("dynacast test [{}]: {}", label, states.join(", "));
        if expected {
            for (sid, _) in tracks.iter().filter(|(sid, _)| !active_sids.contains(sid)) {
                log::info!("dynacast test [{}]: track {} is not being published", label, sid);
            }
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!(
                "dynacast test [{}]: timed out waiting for requested publishing state: {}",
                label,
                states.join(", ")
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
    let layers = wait_for_layers(&pub_video_track, "baseline", Duration::from_secs(15), |layers| {
        layers.len() > 1 && layers.iter().all(|layer| layer.active)
    })
    .await?;
    log::info!("dynacast baseline layers: {:?}", layers);
    assert!(layers.len() > 1, "expected multiple simulcast layers, got {}", layers.len());

    // --- Request LOW quality: SFU should tell publisher to deactivate Medium and High ---
    log::info!("dynacast test: requesting LOW quality");
    sub_publication.set_video_quality(VideoQuality::Low);

    let layers =
        wait_for_layers(&pub_video_track, "after LOW request", Duration::from_secs(30), |layers| {
            let low_active = layers.iter().any(|layer| layer.quality == "Low" && layer.active);
            let high_inactive =
                layers.iter().filter(|layer| layer.quality != "Low").all(|layer| !layer.active);
            low_active && high_inactive
        })
        .await?;
    log::info!("dynacast layers after LOW request: {:?}", layers);
    assert!(
        layers.iter().any(|layer| layer.quality == "Low" && layer.active),
        "expected Low layer to be active, got {:?}",
        layers
    );
    assert!(
        layers.iter().filter(|layer| layer.quality != "Low").all(|layer| !layer.active),
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
        |layers| layers.len() > 1 && layers.iter().all(|layer| layer.active),
    )
    .await?;
    log::info!("dynacast layers after HIGH request: {:?}", layers);
    assert!(
        layers.iter().all(|layer| layer.active),
        "expected all layers active after HIGH request, got {:?}",
        layers
    );

    Ok(())
}

/// Verifies that dynacast only publishes video tracks requested by subscribers.
///
/// A single publisher publishes three simulcast VP8 video tracks while two subscribers manually
/// subscribe to one track each. The subscribers rotate through the three tracks, leaving a
/// different track without subscribers on each cycle; the publisher should disable all layers for
/// that unrequested track.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_dynacast_multiple_subscribers_only_publish_requested_tracks() -> Result<()> {
    let mut pub_room_opts = RoomOptions::default();
    pub_room_opts.dynacast = true;

    let mut sub_room_opts = RoomOptions::default();
    sub_room_opts.dynacast = true;
    sub_room_opts.auto_subscribe = false;

    let pub_options = TestRoomOptions { room: pub_room_opts, ..Default::default() };
    let sub_options = TestRoomOptions { room: sub_room_opts, ..Default::default() };

    let mut rooms =
        test_rooms_with_options([pub_options, sub_options.clone(), sub_options]).await?;
    let (pub_room, _pub_events) = rooms.remove(0);
    let (sub1_room, mut sub1_events) = rooms.remove(0);
    let (sub2_room, mut sub2_events) = rooms.remove(0);

    let pub_room = Arc::new(pub_room);
    let mut solid_tracks = Vec::new();
    let mut track_sids: Vec<TrackSid> = Vec::new();

    for (index, luma) in [64, 128, 192].into_iter().enumerate() {
        let solid_params = SolidColorParams { width: 1280, height: 720, luma };
        let mut solid_track = SolidColorTrack::new(pub_room.clone(), solid_params);
        solid_track.publish(VideoCodec::VP8, true).await?;

        let published_tracks = wait_for_publisher_video_tracks(
            &pub_room,
            index + 1,
            &format!("publish track {}", index + 1),
            Duration::from_secs(15),
        )
        .await?;
        let Some(new_sid) = published_tracks
            .keys()
            .find(|sid| !track_sids.iter().any(|published_sid| published_sid == *sid))
        else {
            return Err(anyhow!("No new track SID found after publishing track {}", index + 1));
        };
        log::info!("dynacast multi: published track {} as {}", index + 1, new_sid);
        track_sids.push(new_sid.clone());
        solid_tracks.push(solid_track);
    }

    let published_tracks = wait_for_publisher_video_tracks(
        &pub_room,
        3,
        "all published tracks",
        Duration::from_secs(5),
    )
    .await?;
    let publisher_tracks: Vec<_> = track_sids
        .iter()
        .map(|sid| {
            let track = published_tracks
                .get(sid)
                .ok_or_else(|| anyhow!("Missing local video track {}", sid))?;
            Ok((sid.clone(), track.clone()))
        })
        .collect::<Result<_>>()?;

    let sub1_publications = wait_for_remote_publications(
        &sub1_room,
        &track_sids,
        "subscriber 1",
        Duration::from_secs(15),
    )
    .await?;
    let sub2_publications = wait_for_remote_publications(
        &sub2_room,
        &track_sids,
        "subscriber 2",
        Duration::from_secs(15),
    )
    .await?;

    for (cycle_index, (sub1_index, sub2_index, inactive_index)) in
        [(0, 1, 2), (1, 2, 0), (2, 0, 1)].into_iter().enumerate()
    {
        let sub1_sid = &track_sids[sub1_index];
        let sub2_sid = &track_sids[sub2_index];
        let inactive_sid = &track_sids[inactive_index];
        let label = format!("cycle {}", cycle_index + 1);

        log::info!(
            "dynacast multi [{}]: subscriber 1 -> {}, subscriber 2 -> {}, unrequested -> {}",
            label,
            sub1_sid,
            sub2_sid,
            inactive_sid
        );

        set_only_subscribed(&sub1_publications, &mut sub1_events, sub1_sid, &label).await?;
        set_only_subscribed(&sub2_publications, &mut sub2_events, sub2_sid, &label).await?;

        wait_for_requested_tracks_only(
            &publisher_tracks,
            &[sub1_sid.clone(), sub2_sid.clone()],
            &label,
            Duration::from_secs(30),
        )
        .await?;
    }

    for solid_track in &mut solid_tracks {
        solid_track.unpublish().await?;
    }

    Ok(())
}
