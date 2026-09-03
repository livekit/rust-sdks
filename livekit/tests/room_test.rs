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
    chrono::{TimeDelta, TimeZone, Utc},
    common::test_rooms,
    libwebrtc::prelude::PeerConnectionState,
    livekit::{ConnectionState, ParticipantKind, RoomEvent},
    std::time::Duration,
    tokio::time::{self, timeout},
};

mod common;

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_connect() -> Result<()> {
    let (room, _) = test_rooms(1).await?.pop().unwrap();

    assert_eq!(room.connection_state(), ConnectionState::Connected);
    assert!(room.name().starts_with("test_room_"));
    assert!(room.remote_participants().is_empty());

    let creation_time = Utc.timestamp_millis_opt(room.creation_time()).unwrap();
    assert!(creation_time.signed_duration_since(Utc::now()).abs() <= TimeDelta::seconds(10));

    let local_participant = room.local_participant();
    assert!(local_participant.sid().as_str().starts_with("PA_"));
    assert_eq!(local_participant.identity().as_str(), "p0");
    assert_eq!(local_participant.name(), "Participant 0");
    assert_eq!(local_participant.kind(), ParticipantKind::Standard);

    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_connect_multiple() -> Result<()> {
    let mut rooms = test_rooms(2).await?;

    let (second, _) = rooms.pop().unwrap();
    let (first, _) = rooms.pop().unwrap();

    assert_eq!(first.name(), second.name(), "Participants are in different rooms");

    assert!(second.remote_participants().get(&first.local_participant().identity()).is_some());
    assert!(first.remote_participants().get(&second.local_participant().identity()).is_some());

    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_participant_disconnect() -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (second, _) = rooms.pop().unwrap();
    let second_sid = second.local_participant().sid();
    let second_name = second.local_participant().name();

    let (_, mut first_event_rx) = rooms.pop().unwrap();

    tokio::spawn(async move {
        time::sleep(Duration::from_millis(400)).await;
        second.close().await?;
        Ok(())
    });

    let wait_for_disconnected = async move {
        while let Some(event) = first_event_rx.recv().await {
            let RoomEvent::ParticipantDisconnected(participant) = event else { continue };
            assert_eq!(participant.sid(), second_sid);
            assert_eq!(participant.name(), second_name);
            break;
        }
        Ok(())
    };
    timeout(Duration::from_secs(15), wait_for_disconnected).await??;
    Ok(())
}

/// A cancelled `close()` must still have closed the peer connections.
///
/// `SessionInner::close` ends with two unbounded awaits, so a caller bounding it with a
/// timeout used to drop the future before the transports were closed — and nothing else
/// closes them, so their ICE sockets stayed bound for the process's lifetime. Cancelling at
/// the first `.await` makes this deterministic: the assertion holds only if the close ran
/// before the future suspended.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_cancelled_close_still_closes_peer_connections() -> Result<()> {
    let (room, _) = test_rooms(1).await?.pop().unwrap();
    assert_eq!(
        room.publisher_connection_state(),
        PeerConnectionState::Connected,
        "publisher should be connected before teardown, or the assertion below proves nothing"
    );

    // Poll the close exactly once, then drop it. `Duration::ZERO` elapses on the first poll,
    // so `timeout` yields `Err` the moment the inner future suspends — cancelling it at the
    // earliest possible point rather than at some arbitrary later one.
    let cancelled = timeout(Duration::ZERO, room.close()).await;
    assert!(cancelled.is_err(), "close should have been cancelled at its first suspension point");

    assert_eq!(
        room.publisher_connection_state(),
        PeerConnectionState::Closed,
        "a cancelled close must not leave the publisher transport open: its ICE sockets are \
         only released by PeerConnection::close, so anything else leaks them permanently"
    );
    Ok(())
}
