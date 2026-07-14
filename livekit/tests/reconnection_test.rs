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

//! Reconnection Tests
//!
//! Exercises the engine reconnection paths end-to-end against a running
//! `livekit-server --dev` (or LiveKit Cloud), via [`Room::simulate_scenario`]:
//!
//! - `SignalReconnect` drives the lightweight *resume* path.
//! - `FullReconnect` asks the server to issue `LeaveRequest{Reconnect}`, forcing
//!   a *full* reconnect (new session, republish).
//!
//! Both should surface `Reconnecting` then `Reconnected` and return the room to
//! `Connected`. These guard the lifecycle-event and recovery behaviour the
//! engine's reconnect loop is responsible for.
//!
//! Environment variables (same as the other e2e suites):
//! - LIVEKIT_URL (default ws://localhost:7880)
//! - LIVEKIT_API_KEY (default "devkey")
//! - LIVEKIT_API_SECRET (default "secret")
//!
//! Run:
//!   cargo test -p livekit --features "__lk-e2e-test,native-tls" --test reconnection_test -- --nocapture

#![cfg(feature = "__lk-e2e-test")]

#[cfg(feature = "__lk-e2e-test")]
use {
    anyhow::{anyhow, bail, Result},
    common::test_rooms,
    libwebrtc::native::create_random_uuid,
    livekit::{ConnectionState, Room, RoomEvent, RoomOptions, SimulateScenario},
    livekit_api::access_token::{AccessToken, VideoGrants},
    std::{env, net::SocketAddr, time::Duration},
    tokio::{
        net::{TcpListener, TcpStream},
        sync::{mpsc::UnboundedReceiver, watch},
        time::timeout,
    },
};

mod common;

/// Drives a reconnection via `scenario` and asserts the room reports
/// `Reconnecting`, then `Reconnected`, and ends up `Connected` again.
#[cfg(feature = "__lk-e2e-test")]
async fn assert_recovers(
    room: Room,
    mut events: UnboundedReceiver<RoomEvent>,
    scenario: SimulateScenario,
) -> Result<()> {
    assert_eq!(room.connection_state(), ConnectionState::Connected);

    // Kick off the reconnection. These scenarios return promptly (they close the
    // local signal channel or ask the server to issue a Leave); recovery then
    // proceeds asynchronously and surfaces as room events, which are buffered on
    // an unbounded channel until we observe them below.
    room.simulate_scenario(scenario)
        .await
        .map_err(|e| anyhow!("simulate_scenario failed: {e:?}"))?;

    // Expect Reconnecting, then Reconnected. Ignore unrelated events in between.
    let observe = async {
        let mut saw_reconnecting = false;
        while let Some(event) = events.recv().await {
            match event {
                RoomEvent::Reconnecting => saw_reconnecting = true,
                RoomEvent::Reconnected => {
                    if !saw_reconnecting {
                        bail!("received Reconnected without a preceding Reconnecting");
                    }
                    return Ok(());
                }
                RoomEvent::Disconnected { reason } => {
                    bail!("room disconnected during reconnection: {reason:?}");
                }
                _ => {}
            }
        }
        bail!("event stream ended before the room reconnected");
    };

    timeout(Duration::from_secs(30), observe).await??;

    assert_eq!(
        room.connection_state(),
        ConnectionState::Connected,
        "room should be Connected after recovery"
    );
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_signal_reconnect_resumes() -> Result<()> {
    let (room, events) = test_rooms(1).await?.pop().unwrap();
    assert_recovers(room, events, SimulateScenario::SignalReconnect).await
}

// `FullReconnect` forces a full reconnect (new session, republish) — driven
// client-side, so it does not depend on the server echoing a leave.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_full_reconnect_recovers() -> Result<()> {
    let (room, events) = test_rooms(1).await?.pop().unwrap();
    assert_recovers(room, events, SimulateScenario::FullReconnect).await
}

// The server drops the signalling link during the resume, so the resume cannot
// complete and the engine must escalate to a full reconnect. Recovery here is
// only possible via that escalation, so a successful Reconnecting → Reconnected
// exercises the resume→full path (and the Restarting emitted on escalation,
// which drives the Room's remote-participant cleanup before the full reconnect).
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_resume_failure_escalates_to_full_reconnect() -> Result<()> {
    let (room, events) = test_rooms(1).await?.pop().unwrap();
    assert_recovers(room, events, SimulateScenario::DisconnectSignalOnResume).await
}

// A participant disconnects while our signal link is down: the SFU broadcast
// its DISCONNECTED update into our dead socket, so it is lost. (Modern OSS
// servers replay recent disconnects on resume from a bounded per-connection
// cache — sendDisconnectUpdatesForReconnect — but that is best-effort: it is
// an LRU keyed off a last-seen-signal heuristic and does not exist where the
// resume is served without the previous connection's state, as observed on
// LiveKit Cloud. The `drop_disconnected_updates` fault simulates that loss.)
// On resume the server sends a full participant snapshot; the room must
// notice the participant is absent from it and synthesize
// ParticipantDisconnected — otherwise `remote_participants` keeps a ghost
// entry forever and the application never learns the participant left.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_resume_synthesizes_disconnect_for_participant_that_left() -> Result<()> {
    let api_key = env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| "devkey".into());
    let api_secret = env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| "secret".into());
    let server_url = env::var("LIVEKIT_URL").unwrap_or_else(|_| "ws://localhost:7880".into());

    let room_name = format!("test_room_{}", create_random_uuid());
    let token = |identity: &str| {
        AccessToken::with_api_key(&api_key, &api_secret)
            .with_ttl(Duration::from_secs(30 * 60))
            .with_grants(VideoGrants { room_join: true, room: room_name.clone(), ..Default::default() })
            .with_identity(identity)
            .with_name(identity)
            .to_jwt()
    };

    // Three participants: the observer (whose view we assert on), the leaver
    // (disconnects mid-test), and a witness who stays — proving the
    // reconciliation only removes participants who actually left.
    let (observer, mut events) =
        Room::connect(&server_url, &token("observer")?, RoomOptions::default()).await?;
    let (leaver, _leaver_events) =
        Room::connect(&server_url, &token("leaver")?, RoomOptions::default()).await?;
    let (_witness, mut witness_events) =
        Room::connect(&server_url, &token("witness")?, RoomOptions::default()).await?;

    // Wait until the observer knows both remote participants.
    let saw_joins = async {
        let mut pending: std::collections::HashSet<&str> =
            ["leaver", "witness"].into_iter().collect();
        while let Some(event) = events.recv().await {
            if let RoomEvent::ParticipantConnected(p) = event {
                pending.remove(p.identity().as_str());
                if pending.is_empty() {
                    return Ok(());
                }
            }
        }
        bail!("event stream ended before all participants joined");
    };
    timeout(Duration::from_secs(15), saw_joins).await??;
    let sid_before = observer.local_participant().sid();

    // From here on the observer never receives an explicit DISCONNECTED
    // entry, exactly as if every delivery (and resume-time replay) attempt of
    // the leaver's disconnect had been lost with a dead connection.
    observer.drop_disconnected_updates(true);

    // The leaver leaves; the witness confirms the server processed it (so the
    // resume snapshot below no longer contains the leaver).
    leaver.close().await?;
    let saw_leave = async {
        while let Some(event) = witness_events.recv().await {
            if let RoomEvent::ParticipantDisconnected(p) = event {
                if p.identity().as_str() == "leaver" {
                    return Ok(());
                }
            }
        }
        bail!("witness event stream ended before the leaver's disconnect was broadcast");
    };
    timeout(Duration::from_secs(15), saw_leave).await??;

    // Sanity: the observer still holds the ghost entry.
    assert!(
        observer.remote_participants().keys().any(|identity| identity.as_str() == "leaver"),
        "observer should still hold the (stale) leaver before resuming"
    );

    // Resume the signal connection. The post-resume participant snapshot no
    // longer lists the leaver, and the reconciliation at the end of the
    // resume must synthesize its ParticipantDisconnected.
    observer
        .simulate_scenario(SimulateScenario::SignalReconnect)
        .await
        .map_err(|e| anyhow!("simulate_scenario failed: {e:?}"))?;

    let observe = async {
        let mut reconnected = false;
        let mut leaver_disconnected = false;
        while let Some(event) = events.recv().await {
            match event {
                RoomEvent::Reconnected => reconnected = true,
                RoomEvent::ParticipantDisconnected(p) => {
                    if p.identity().as_str() == "leaver" {
                        leaver_disconnected = true;
                    } else {
                        bail!(
                            "reconciliation disconnected a participant that never left: {}",
                            p.identity()
                        );
                    }
                }
                RoomEvent::Disconnected { reason } => {
                    bail!("observer disconnected during resume: {reason:?}");
                }
                _ => {}
            }
            if reconnected && leaver_disconnected {
                return Ok(());
            }
        }
        bail!("event stream ended before ParticipantDisconnected was synthesized");
    };
    timeout(Duration::from_secs(30), observe).await??;

    let remaining = observer.remote_participants();
    assert!(
        !remaining.keys().any(|identity| identity.as_str() == "leaver"),
        "remote_participants must not keep a ghost entry for a participant that left"
    );
    assert!(
        remaining.keys().any(|identity| identity.as_str() == "witness"),
        "reconciliation must keep participants that are still in the room"
    );
    // An unchanged local sid proves this recovered via resume — a full
    // reconnect (which clears participants anyway) would assign a new one and
    // would not exercise the resume-time reconciliation under test.
    assert_eq!(
        observer.local_participant().sid(),
        sid_before,
        "expected the lightweight resume path, but the session was fully restarted"
    );
    Ok(())
}

/// A minimal TCP proxy in front of the signalling server that can be killed.
/// Sending `true` on the returned channel closes the in-flight connection and
/// stops accepting, so the client's reconnect attempts all fail (connection
/// refused). Used to drive the engine's reconnect loop to exhaustion.
#[cfg(feature = "__lk-e2e-test")]
async fn start_killable_proxy(target_host_port: String) -> (SocketAddr, watch::Sender<bool>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind proxy");
    let addr = listener.local_addr().expect("proxy addr");
    let (kill_tx, kill_rx) = watch::channel(false);

    tokio::spawn(async move {
        loop {
            let mut kr = kill_rx.clone();
            tokio::select! {
                _ = kr.changed() => break, // kill: stop accepting; dropping `listener` refuses new connects
                accepted = listener.accept() => {
                    let Ok((mut inbound, _)) = accepted else { break };
                    let target = target_host_port.clone();
                    let mut kr2 = kill_rx.clone();
                    tokio::spawn(async move {
                        if let Ok(mut outbound) = TcpStream::connect(&target).await {
                            tokio::select! {
                                _ = kr2.changed() => {} // kill: drop both streams, severing the client link
                                _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound) => {}
                            }
                        }
                    });
                }
            }
        }
    });

    (addr, kill_tx)
}

// When every reconnect attempt fails, the engine must exhaust its bounded
// retries and emit Disconnected — it must NOT stay stuck in Reconnecting
// forever. We connect through a killable proxy, kill it, and assert the room
// reaches Disconnected after a reconnection was attempted. (The reason is
// UnknownReason here because a dropped signal link carries no richer cause; the
// #2b improvement surfaces a meaningful cause only when the triggering event
// has one.) Slow by design: it waits out the full bounded backoff sequence.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_reconnect_exhaustion_disconnects() -> Result<()> {
    let api_key = env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| "devkey".into());
    let api_secret = env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| "secret".into());
    let server_url = env::var("LIVEKIT_URL").unwrap_or_else(|_| "ws://localhost:7880".into());

    // Derive host:port for raw TCP forwarding (the WS upgrade rides over it).
    let target = server_url
        .split("://")
        .last()
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("localhost:7880")
        .to_string();

    let (proxy_addr, kill) = start_killable_proxy(target).await;
    let proxy_url = format!("ws://{proxy_addr}");

    let room_name = format!("test_room_{}", create_random_uuid());
    let token = AccessToken::with_api_key(&api_key, &api_secret)
        .with_ttl(Duration::from_secs(30 * 60))
        .with_grants(VideoGrants { room_join: true, room: room_name, ..Default::default() })
        .with_identity("p0")
        .with_name("Participant 0")
        .to_jwt()?;

    let (room, mut events) = Room::connect(&proxy_url, &token, RoomOptions::default()).await?;
    assert_eq!(room.connection_state(), ConnectionState::Connected);

    // Sever the link and refuse all reconnects.
    kill.send(true).ok();

    let observe = async {
        let mut saw_reconnecting = false;
        while let Some(event) = events.recv().await {
            match event {
                RoomEvent::Reconnecting => saw_reconnecting = true,
                RoomEvent::Disconnected { reason } => {
                    if !saw_reconnecting {
                        bail!("disconnected without attempting reconnection first");
                    }
                    return Ok(reason);
                }
                _ => {}
            }
        }
        bail!("event stream ended before the room reported Disconnected");
    };

    // Generous timeout: the engine works through its full bounded backoff before
    // giving up.
    let _reason = timeout(Duration::from_secs(90), observe).await??;

    assert_eq!(
        room.connection_state(),
        ConnectionState::Disconnected,
        "room must reach Disconnected after reconnection is exhausted, not hang in Reconnecting"
    );
    Ok(())
}
