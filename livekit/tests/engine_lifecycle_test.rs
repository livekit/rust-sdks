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
    anyhow::{anyhow, bail, Context, Ok, Result},
    common::{test_engine, TestEnvironment},
    libwebrtc::{native::create_random_uuid, peer_connection_factory::IceTransportsType},
    livekit::{Room, RoomOptions},
    livekit_api::services::room::RoomClient,
    livekit_token::{AccessToken, VideoGrants},
    std::time::Duration,
    tokio::time::{self, timeout},
};

mod common;

/// Dropping the engine without an explicit `close()` must still release its internals.
///
/// The engine task is owned by the engine (its `JoinHandle` is stored in `EngineHandle`),
/// so it must not hold a strong reference back to what owns it. If it does, the task and
/// the engine keep each other alive and nothing short of `close()` can break the cycle,
/// leaking the session, both peer connections, the signal client and its websocket.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_drop_without_close_releases_engine() -> Result<()> {
    let engine = test_engine().await?;
    let engine_dropped = engine.drop_probe();

    drop(engine);

    // Teardown unwinds across several tasks, so poll for release rather than
    // asserting immediately or sleeping for an arbitrary fixed duration.
    let released = timeout(Duration::from_secs(10), async {
        while !engine_dropped() {
            time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .is_ok();

    assert!(released, "engine internals retained after drop");
    Ok(())
}

/// Cancelling `connect` while it waits for the peer connection must end the session.
///
/// Nothing was returned to the caller, so nothing can close it afterwards: the dropped
/// future is the only owner. If signalling outlives it, the signal task keeps answering
/// pings and the server keeps the participant in the room.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_cancelled_connect_leaves_the_room() -> Result<()> {
    let test_env = TestEnvironment::from_env_or_defaults();
    let room_name = format!("test_room_{}", create_random_uuid());
    let token = AccessToken::with_api_key(&test_env.api_key, &test_env.api_secret)
        .with_grants(VideoGrants { room_join: true, room: room_name.clone(), ..Default::default() })
        .with_identity("p0")
        .to_jwt()
        .context("Failed to generate JWT")?;

    let mut options = RoomOptions::default();
    // Relay only, and the dev server offers no TURN: ICE can never connect.
    options.rtc_config.ice_transport_type = IceTransportsType::Relay;
    options.join_retries = 0;
    let server_url = test_env.server_url.clone();
    let connect = tokio::spawn(async move { Room::connect(&server_url, &token, options).await });

    let http_url = test_env.server_url.replacen("ws", "http", 1);
    let rooms = RoomClient::with_api_key(&http_url, &test_env.api_key, &test_env.api_secret);
    timeout(Duration::from_secs(10), async {
        while rooms.list_participants(&room_name).await.unwrap_or_default().is_empty() {
            time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .map_err(|_| anyhow!("the participant never joined"))?;
    // The server lists the participant before the client has read the join response. Give it
    // time to, so that the cancel lands on a session that is waiting for ICE.
    time::sleep(Duration::from_millis(500)).await;
    if connect.is_finished() {
        bail!("connect finished before it could be cancelled");
    }
    connect.abort();
    assert!(connect.await.is_err_and(|err| err.is_cancelled()));

    // Well under the time the server takes to give up on a participant on its own.
    timeout(Duration::from_secs(5), async {
        while !rooms.list_participants(&room_name).await.unwrap_or_default().is_empty() {
            time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| anyhow!("participant left behind after the cancelled connect"))?;
    Ok(())
}
