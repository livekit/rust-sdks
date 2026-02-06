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

use anyhow::{Context, Result};
use chrono::Utc;
use futures_util::future::try_join_all;
use libwebrtc::native::create_random_uuid;
use livekit::{Room, RoomEvent, RoomOptions};
use livekit_api::access_token::{AccessToken, VideoGrants};
use std::{env, time::Duration};
use tokio::{
    sync::mpsc::UnboundedReceiver,
    time::{self, timeout},
};

pub mod audio;

struct TestEnvironment {
    api_key: String,
    api_secret: String,
    server_url: String,
}

impl TestEnvironment {
    /// Reads API key, secret, and server URL from the environment, using the
    /// development defaults for values that are not present.
    pub fn from_env_or_defaults() -> Self {
        Self {
            api_key: env::var("LIVEKIT_API_KEY").unwrap_or("devkey".into()),
            api_secret: env::var("LIVEKIT_API_SECRET").unwrap_or("secret".into()),
            server_url: env::var("LIVEKIT_URL").unwrap_or("http://localhost:7880".into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestRoomOptions {
    /// Grants for the generated token.
    pub grants: VideoGrants,
    /// Options used for creating the [`Room`].
    pub room: RoomOptions,
}

impl Default for TestRoomOptions {
    fn default() -> Self {
        Self {
            grants: VideoGrants { room_join: true, ..Default::default() },
            room: Default::default(),
        }
    }
}

impl From<RoomOptions> for TestRoomOptions {
    fn from(room: RoomOptions) -> Self {
        Self { room, ..Default::default() }
    }
}

impl From<VideoGrants> for TestRoomOptions {
    fn from(grants: VideoGrants) -> Self {
        Self { grants, ..Default::default() }
    }
}

/// Creates the specified number of connections to a shared room for testing.
pub async fn test_rooms(count: usize) -> Result<Vec<(Room, UnboundedReceiver<RoomEvent>)>> {
    test_rooms_with_options((0..count).map(|_| TestRoomOptions::default())).await
}

/// Creates multiple connections to a shared room for testing, one for each configuration.
pub async fn test_rooms_with_options(
    options: impl IntoIterator<Item = TestRoomOptions>,
) -> Result<Vec<(Room, UnboundedReceiver<RoomEvent>)>> {
    let test_env = TestEnvironment::from_env_or_defaults();
    let room_name = format!("test_room_{}", create_random_uuid());

    let tokens = options
        .into_iter()
        .enumerate()
        .map(|(id, mut options)| -> Result<(String, RoomOptions)> {
            options.grants.room = room_name.clone();

            let token = AccessToken::with_api_key(&test_env.api_key, &test_env.api_secret)
                .with_ttl(Duration::from_secs(30 * 60)) // 30 minutes
                .with_grants(options.grants)
                .with_identity(&format!("p{}", id))
                .with_name(&format!("Participant {}", id))
                .to_jwt()
                .context("Failed to generate JWT")?;
            Ok((token, options.room))
        })
        .collect::<Result<Vec<_>>>()?;

    let count = tokens.len();
    let rooms = try_join_all(tokens.into_iter().map(|(token, options)| {
        let server_url = test_env.server_url.clone();
        async move {
            Room::connect(&server_url, &token, options).await.context("Failed to connect to room")
        }
    }))
    .await?;

    // Wait for participant visibility across all room connections. When using a
    // local SFU, this takes significantly longer and can lead to intermittently failing tests.
    let all_connected_time = Utc::now();
    let wait_participant_visibility = async {
        while rooms.iter().any(|(room, _)| room.remote_participants().len() != count - 1) {
            time::sleep(Duration::from_millis(10)).await;
        }
        log::info!("All participants visible after {}", Utc::now() - all_connected_time);
    };
    timeout(Duration::from_secs(5), wait_participant_visibility)
        .await
        .context("Not all participants became visible")?;

    Ok(rooms)
}
