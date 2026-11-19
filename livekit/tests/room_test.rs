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
    chrono::{TimeDelta, TimeZone, Utc},
    common::{test_rooms, test_rooms_with_options},
    libwebrtc::{
        audio_source::native::NativeAudioSource,
        prelude::{
            AudioSourceOptions, PeerConnectionState, RtcAudioSource, RtcVideoSource,
            VideoResolution,
        },
        video_source::native::NativeVideoSource,
    },
    livekit::{
        e2ee::{
            key_provider::{KeyProvider, KeyProviderOptions},
            EncryptionType,
        },
        options::TrackPublishOptions,
        track::{LocalAudioTrack, LocalTrack, LocalVideoTrack},
        ConnectionState, E2eeOptions, ParticipantKind, RoomEvent, RoomOptions,
    },
    std::{
        future::{poll_fn, Future},
        task::Poll,
        time::Duration,
    },
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

/// Closing and dropping a room must release the internal room session.
///
/// Stored callbacks owned by the room must not strongly capture the room session. Such a
/// reference cycle prevents the session, RTC engine, and WebRTC runtime from being destroyed.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_close_releases_room_session() -> Result<()> {
    let (room, events) = test_rooms(1).await?.pop().unwrap();
    let session_dropped = room.drop_probe();

    drop(events);
    room.close().await?;
    drop(room);

    assert!(session_dropped(), "room callbacks retained the room session after close");
    Ok(())
}

/// `close()` must tear down remote participants, not only the local one.
///
/// Every remote publication registers callbacks that hold the participant which owns
/// them: `on_subscribed`/`on_unsubscribed` capture the `RemoteParticipant`, and
/// `on_muted`/`on_unmuted` capture it through the publication map. Those are reference
/// cycles, so a remote participant (and the `Arc<RtcEngine>` it holds) outlives the
/// room unless teardown unregisters them.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_close_releases_remote_participants() -> Result<()> {
    check_remote_participant_release(false, false).await
}

/// Encrypted close must not enqueue participant notifications during teardown.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_close_releases_encrypted_remote_participants() -> Result<()> {
    check_remote_participant_release(true, false).await
}

/// A cancelled `close()` must still release the remote participants and the room session.
///
/// `close` takes the room handle before it does anything else, so a caller that bounds it
/// with a timeout and drops the future cannot retry: a later call returns `AlreadyClosed`.
/// If the participant teardown sat behind the engine shutdown and the task joins, dropping
/// the future at any of those suspension points would skip it permanently, and the publication
/// callbacks would keep holding the participant and its `RtcEngine`.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_cancelled_close_releases_remote_participants() -> Result<()> {
    check_remote_participant_release(false, true).await
}

/// Cancellation must also release encrypted publications and their subscribed tracks.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_cancelled_close_releases_encrypted_remote_participants() -> Result<()> {
    check_remote_participant_release(true, true).await
}

/// Exercises remote callback ownership with real subscriptions and optional cancellation.
#[cfg(feature = "__lk-e2e-test")]
async fn check_remote_participant_release(encrypted: bool, cancel_close: bool) -> Result<()> {
    let options = || {
        let mut options = RoomOptions::default();
        options.encryption = encrypted.then(|| E2eeOptions {
            encryption_type: EncryptionType::Gcm,
            key_provider: KeyProvider::with_shared_key(
                KeyProviderOptions::default(),
                b"lifecycle-test-key".to_vec(),
            ),
        });
        options
    };
    let mut rooms = test_rooms_with_options([options().into(), options().into()]).await?;
    let (pub_room, _) = rooms.pop().unwrap();
    let (sub_room, mut sub_events) = rooms.pop().unwrap();

    // The review reproduced encrypted teardown with a video track.
    let track = if encrypted {
        let source = NativeVideoSource::new(VideoResolution { width: 64, height: 64 }, false);
        LocalTrack::Video(LocalVideoTrack::create_video_track(
            "video",
            RtcVideoSource::Native(source),
        ))
    } else {
        let source = NativeAudioSource::new(AudioSourceOptions::default(), 48_000, 1, 1000);
        LocalTrack::Audio(LocalAudioTrack::create_audio_track(
            "audio",
            RtcAudioSource::Native(source),
        ))
    };
    pub_room.local_participant().publish_track(track, TrackPublishOptions::default()).await?;

    // The cycles only exist once the remote participant has a publication. A subscribed
    // track registers both participant and publication callbacks.
    let (participant_dropped, publication_dropped) = timeout(Duration::from_secs(10), async {
        while let Some(event) = sub_events.recv().await {
            if let RoomEvent::TrackSubscribed { participant, publication, .. } = event {
                assert_eq!(participant.is_encrypted(), encrypted);
                assert_eq!(publication.encryption_type() == EncryptionType::Gcm, encrypted);
                return Ok((participant.drop_probe(), publication.drop_probe()));
            }
        }
        Err(anyhow!("event channel closed before TrackSubscribed"))
    })
    .await??;

    // Discard setup events so the checks below only inspect events arriving during close.
    while sub_events.try_recv().is_ok() {}
    // `sub_events` is deliberately kept alive across the close. Teardown must not dispatch
    // anything carrying the participant: a queued event holds strong clones, so a receiver
    // the application has stopped polling would pin the participant just as effectively as
    // the callbacks did.
    let session_dropped = sub_room.drop_probe();
    if cancel_close {
        let mut close = Box::pin(sub_room.close());
        // Poll exactly once and cancel at the first suspension, without giving background
        // tasks an opportunity to run between that suspension and cancellation.
        let first_poll = poll_fn(|cx| Poll::Ready(close.as_mut().poll(cx))).await;
        assert!(first_poll.is_pending(), "close completed without suspending");
        drop(close);
    } else {
        sub_room.close().await?;
    }
    drop(sub_room);

    // Connection-quality and other legitimate background events can hold participant clones.
    // Inspect notifications first, then release their payloads before checking destruction.
    timeout(Duration::from_secs(10), async {
        loop {
            while let std::result::Result::Ok(event) = sub_events.try_recv() {
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
            if session_dropped() && participant_dropped() && publication_dropped() {
                break;
            }
            time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| {
        anyhow!(
            "retained after close: session={}, participant={}, publication={}",
            !session_dropped(),
            !participant_dropped(),
            !publication_dropped()
        )
    })?;

    // The publisher stays connected throughout subscriber teardown.
    pub_room.close().await?;
    Ok(())
}

/// `close()` must leave the room Disconnected and report it, every time.
///
/// The room's event task used to be the only place that applied the engine's `Disconnected`
/// event, and it races that event against the close signal `close()` sends right after; when
/// both are ready the close can win and the event is dropped, leaving the room in its previous
/// state. Each iteration hits that race with roughly even odds.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_close_leaves_room_disconnected() -> Result<()> {
    for _ in 0..5 {
        let (room, mut events) = test_rooms(1).await?.pop().unwrap();
        room.close().await?;

        assert_eq!(room.connection_state(), ConnectionState::Disconnected);
        let mut disconnected = false;
        while let Some(event) = events.try_recv().ok() {
            disconnected |= matches!(
                event,
                RoomEvent::Disconnected { reason: livekit::DisconnectReason::ClientInitiated }
            );
        }
        assert!(disconnected, "close() did not emit RoomEvent::Disconnected");
    }
    Ok(())
}
