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
    livekit::{ConnectionState, Room, RoomEvent, SimulateScenario},
    std::time::Duration,
    tokio::{sync::mpsc::UnboundedReceiver, time::timeout},
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
