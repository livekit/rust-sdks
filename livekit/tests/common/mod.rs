use anyhow::{Context, Result};
use futures_util::future::try_join_all;
use libwebrtc::native::create_random_uuid;
use livekit::{Room, RoomEvent, RoomOptions};
use livekit_api::access_token::{AccessToken, VideoGrants};
use std::{env, time::Duration};
use tokio::sync::mpsc::UnboundedReceiver;

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

/// Creates the specified number of connections to a shared room for testing.
pub async fn test_rooms(count: usize) -> Result<Vec<(Room, UnboundedReceiver<RoomEvent>)>> {
    let test_env = TestEnvironment::from_env_or_defaults();
    let room_name = format!("test_room_{}", create_random_uuid());

    let tokens = (0..count)
        .into_iter()
        .map(|id| -> Result<String> {
            let grants =
                VideoGrants { room_join: true, room: room_name.clone(), ..Default::default() };
            Ok(AccessToken::with_api_key(&test_env.api_key, &test_env.api_secret)
                .with_ttl(Duration::from_secs(30 * 60)) // 30 minutes
                .with_grants(grants)
                .with_identity(&format!("p{}", id))
                .to_jwt()
                .context("Failed to generate JWT")?)
        })
        .collect::<Result<Vec<_>>>()?;

    let rooms = try_join_all(tokens.into_iter().map(|token| {
        let server_url = test_env.server_url.clone();
        async move {
            let options = RoomOptions::default();
            Room::connect(&server_url, &token, options).await.context("Failed to connect to room")
        }
    }))
    .await?;

    Ok(rooms)
}
