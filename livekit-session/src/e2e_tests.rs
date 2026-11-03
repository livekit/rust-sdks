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

//! End-to-end tests against a running `livekit-server --dev`, with a second
//! room standing in for the agent.
//!
//! Environment variables (the same as `livekit`'s e2e suites):
//! - LIVEKIT_URL (default ws://localhost:7880)
//! - LIVEKIT_API_KEY (default "devkey")
//! - LIVEKIT_API_SECRET (default "secret")
//!
//! Run:
//!   cargo test -p livekit-session --features __lk-e2e-test

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::prelude::{AudioSourceOptions, RtcVideoSource, VideoResolution};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::{SendTextOptions, SimulateScenario, StreamReader, StreamTextOptions, StreamWriter};
use livekit_api::services::room::RoomClient;
use livekit_token::{AccessToken, VideoGrants};
use livekit_token_source::{
    literal, TokenSourceConfigurable, TokenSourceError, TokenSourceFetchOptions,
    TokenSourceLiteral, TokenSourceResponse, TokenSourceResult,
};
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};

use crate::actor::{lock, Shared};
use crate::constants::{
    ATTRIBUTE_AGENT_STATE, ATTRIBUTE_PUBLISH_ON_BEHALF, ATTRIBUTE_SEGMENT_ID,
    ATTRIBUTE_TRANSCRIPTION_FINAL, TOPIC_CHAT, TOPIC_TRANSCRIPTION,
};
use crate::session::Lifecycle;
use crate::*;

const USER: &str = "user";
const AGENT: &str = "agent";
const WAIT: Duration = Duration::from_secs(10);

struct TestEnv {
    url: String,
    api_key: String,
    api_secret: String,
    room: String,
}

impl TestEnv {
    fn new() -> Self {
        // The clock alone can repeat across tests started in parallel.
        static ROOMS: AtomicUsize = AtomicUsize::new(0);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let count = ROOMS.fetch_add(1, Ordering::Relaxed);
        Self {
            url: std::env::var("LIVEKIT_URL").unwrap_or_else(|_| "ws://localhost:7880".into()),
            api_key: std::env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| "devkey".into()),
            api_secret: std::env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| "secret".into()),
            room: format!("session_test_{nanos}_{count}"),
        }
    }

    fn token(&self, identity: &str, customize: impl FnOnce(AccessToken) -> AccessToken) -> String {
        let token = AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_ttl(Duration::from_secs(30 * 60))
            .with_grants(VideoGrants {
                room_join: true,
                room: self.room.clone(),
                can_update_own_metadata: Some(true),
                ..Default::default()
            })
            .with_identity(identity);
        customize(token).to_jwt().expect("Failed to sign token")
    }

    /// Credentials for the session's user, optionally dispatching an agent.
    fn user_source(&self, dispatch: bool) -> TokenSourceLiteral {
        let token = self.token(USER, |token| {
            if !dispatch {
                return token;
            }
            token.with_room_config(livekit_protocol::RoomConfiguration {
                agents: vec![livekit_protocol::RoomAgentDispatch {
                    agent_name: "test-agent".into(),
                    ..Default::default()
                }],
                ..Default::default()
            })
        });
        literal(self.url.clone(), token)
    }

    /// Joins a participant of kind agent reporting `state`.
    async fn join_agent(
        &self,
        identity: &str,
        state: &str,
    ) -> Result<(Room, UnboundedReceiver<RoomEvent>)> {
        let token = self.token(identity, |token| {
            token
                .with_kind("agent")
                .with_attributes([(ATTRIBUTE_AGENT_STATE.to_owned(), state.to_owned())])
        });
        Ok(Room::connect(&self.url, &token, RoomOptions::default()).await?)
    }

    /// Joins an avatar worker: a participant of kind agent publishing on
    /// behalf of `agent`.
    async fn join_avatar_worker(
        &self,
        identity: &str,
        agent: &str,
    ) -> Result<(Room, UnboundedReceiver<RoomEvent>)> {
        let token = self.token(identity, |token| {
            token
                .with_kind("agent")
                .with_attributes([(ATTRIBUTE_PUBLISH_ON_BEHALF.to_owned(), agent.to_owned())])
        });
        Ok(Room::connect(&self.url, &token, RoomOptions::default()).await?)
    }

    async fn join_observer(&self) -> Result<(Room, UnboundedReceiver<RoomEvent>)> {
        let token = self.token("observer", |token| token);
        Ok(Room::connect(&self.url, &token, RoomOptions::default()).await?)
    }
}

async fn set_agent_state(agent: &Room, state: &str) -> Result<()> {
    agent
        .local_participant()
        .set_attributes(HashMap::from([(ATTRIBUTE_AGENT_STATE.to_owned(), state.to_owned())]))
        .await?;
    Ok(())
}

async fn wait_for(
    session: &Session,
    what: &str,
    predicate: impl FnMut(&SessionState) -> bool,
) -> Result<SessionState> {
    let mut state = session.state();
    let reached = timeout(WAIT, state.wait_for(predicate))
        .await
        .map_err(|_| anyhow!("timed out waiting for {what}; last {:?}", current(session)))??;
    Ok(*reached)
}

async fn wait_for_agent(session: &Session, agent_state: AgentState) -> Result<()> {
    wait_for(session, &format!("{agent_state:?}"), |s| s.agent == agent_state).await?;
    Ok(())
}

async fn eventually(what: &str, mut condition: impl FnMut() -> bool) -> Result<()> {
    timeout(WAIT, async {
        while !condition() {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .map_err(|_| anyhow!("timed out waiting for {what}"))
}

fn weak_shared(session: &Session) -> Weak<Shared> {
    Arc::downgrade(&session.inner.shared)
}

fn is_idle(session: &Session) -> bool {
    matches!(*lock(&session.inner.lifecycle), Lifecycle::Idle)
}

fn current(session: &Session) -> SessionState {
    *session.state().borrow()
}

/// Options for a session that stays off the machine's audio devices.
fn text_options() -> SessionOptions {
    SessionOptions::default().with_platform_audio(false)
}
