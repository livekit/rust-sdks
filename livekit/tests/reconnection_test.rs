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
    room.simulate_scenario(scenario).await.map_err(|e| anyhow!("simulate_scenario failed: {e:?}"))?;

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
