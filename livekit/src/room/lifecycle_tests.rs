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

use super::*;
use libwebrtc::{
    prelude::{RtcVideoSource, VideoResolution},
    video_source::native::NativeVideoSource,
};
use livekit_token::{AccessToken, VideoGrants};

/// Connects a single room so synthetic peers have no background server updates.
async fn test_room() -> (Room, mpsc::UnboundedReceiver<RoomEvent>) {
    let key = std::env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| "devkey".into());
    let secret = std::env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| "secret".into());
    let url = std::env::var("LIVEKIT_URL").unwrap_or_else(|_| "http://localhost:7880".into());
    let token = AccessToken::with_api_key(&key, &secret)
        .with_grants(VideoGrants {
            room_join: true,
            room: format!("lifecycle_{}", libwebrtc::native::create_random_uuid()),
            ..Default::default()
        })
        .with_identity("local")
        .to_jwt()
        .unwrap();
    Room::connect(&url, &token, RoomOptions::default()).await.unwrap()
}

/// Describes an encrypted peer unknown to the server, eliminating background-event retention.
fn remote_info() -> proto::ParticipantInfo {
    proto::ParticipantInfo {
        sid: "PA_remote".into(),
        identity: "remote".into(),
        state: proto::participant_info::State::Active as i32,
        tracks: vec![proto::TrackInfo {
            sid: "TR_video".into(),
            r#type: proto::TrackType::Video as i32,
            encryption: proto::encryption::Type::Gcm as i32,
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// Waits for task teardown and verifies destruction of every observed owner.
async fn assert_released(probes: &[&dyn Fn() -> bool]) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while probes.iter().any(|probe| !probe()) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("room, participant, or publication retained after teardown");
}

/// With no background peer events, keeping a receiver undrained must not retain remote state.
#[tokio::test]
async fn close_with_undrained_receiver_releases_encrypted_remote() {
    let (room, mut events) = test_room().await;
    room.inner.handle_participant_update(vec![remote_info()]);
    let participant = room.remote_participants().into_values().next().unwrap();
    let publication = participant.track_publications().into_values().next().unwrap();
    // This synthetic track has no RTP receiver for E2EE setup. Real encrypted subscriptions
    // are covered by room_test; retain the production unsubscribe and encryption callbacks.
    participant.on_track_subscribed(|_, _, _| {});
    let source = NativeVideoSource::new(VideoResolution { width: 64, height: 64 }, false);
    let local = LocalVideoTrack::create_video_track("video", RtcVideoSource::Native(source));
    let track = RemoteVideoTrack::new(publication.sid(), "video".into(), local.rtc_track());
    publication.set_track(Some(RemoteTrack::Video(track)));
    assert!(participant.is_encrypted());

    let participant_dropped = participant.drop_probe();
    let publication_dropped = publication.drop_probe();
    let session_dropped = room.drop_probe();
    drop(participant);
    drop(publication);
    drop(local);
    while events.try_recv().is_ok() {}

    room.close().await.unwrap();
    drop(room);
    // Deliberately do not drain or drop the receiver until destruction is established.
    assert_released(&[&session_dropped, &participant_dropped, &publication_dropped]).await;
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(
                event,
                RoomEvent::TrackUnsubscribed { .. }
                    | RoomEvent::TrackUnpublished { .. }
                    | RoomEvent::ParticipantDisconnected(_)
                    | RoomEvent::ParticipantEncryptionStatusChanged { .. }
            ),
            "unexpected teardown notification: {event:?}"
        );
    }
}

/// An update holding a participant outside the drained map must not recreate ownership cycles.
///
/// Only a completed close is checked here: idling after a cancelled close can crash in
/// `engine_task`, which drops queued transceivers after their peer connection is torn down
/// (<https://github.com/livekit/rust-sdks/issues/1655>). Cancellation is covered by the
/// `test_cancelled_close_*` tests in `room_test`.
#[tokio::test]
async fn update_after_remote_map_drain_releases_participant() {
    let (room, mut events) = test_room().await;
    let mut info = remote_info();
    room.inner.handle_participant_update(vec![info.clone()]);
    // Model an update paused immediately after lookup, before it mutates publications.
    let participant =
        room.inner.get_participant_by_identity(&info.identity.clone().into()).unwrap();
    let participant_dropped = participant.drop_probe();
    let original_publication = participant.track_publications().into_values().next().unwrap();
    let original_dropped = original_publication.drop_probe();
    drop(original_publication);

    // Resume the update after a teardown snapshot has relinquished the map's ownership.
    room.inner.release_remote_participants();
    assert!(participant.track_publications().is_empty());
    info.tracks[0].sid = "TR_late".into();
    participant.update_info(info);
    assert!(participant.track_publications().is_empty(), "late update restored publications");
    // New participants must also be rejected once the teardown snapshot has been taken.
    room.inner.handle_participant_update(vec![remote_info()]);
    assert!(room.remote_participants().is_empty());
    let session_dropped = room.drop_probe();
    room.close().await.unwrap();
    drop(room);
    drop(participant);
    // Update notifications legitimately retain the participant until consumed.
    while events.try_recv().is_ok() {}
    assert_released(&[&session_dropped, &participant_dropped, &original_dropped]).await;
}
