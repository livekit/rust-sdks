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

//! Helpers shared by the end-to-end tests: the black-box ones in
//! `tests/session_test.rs`, and the white-box ones in `src/e2e_tests.rs`,
//! which include this file by path.

// Each of the two test crates uses only some of the helpers.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use livekit::prelude::*;
use livekit_session::{
    AgentActivity, AgentFailure, AgentState, Session, SessionOptions, SessionState,
};
use livekit_token::{AccessToken, VideoGrants};
use livekit_token_source::{literal, TokenSourceLiteral};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{sleep, timeout};

// The attributes an agent sets, spelled out so that the tests pin the wire
// names independently of the crate's own constants.
const ATTRIBUTE_AGENT_STATE: &str = "lk.agent.state";
const ATTRIBUTE_PUBLISH_ON_BEHALF: &str = "lk.publish_on_behalf";

pub const USER: &str = "user";
pub const AGENT: &str = "agent";
pub const WAIT: Duration = Duration::from_secs(10);

pub struct TestEnv {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
    pub room: String,
}

impl TestEnv {
    pub fn new() -> Self {
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

    /// The grants of every token: joining the test's room, and setting
    /// attributes.
    pub fn grants(&self) -> VideoGrants {
        VideoGrants {
            room_join: true,
            room: self.room.clone(),
            can_update_own_metadata: Some(true),
            ..Default::default()
        }
    }

    pub fn token(
        &self,
        identity: &str,
        customize: impl FnOnce(AccessToken) -> AccessToken,
    ) -> String {
        let token = AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_ttl(Duration::from_secs(30 * 60))
            .with_grants(self.grants())
            .with_identity(identity);
        customize(token).to_jwt().expect("Failed to sign token")
    }

    /// Credentials for the session's user, optionally dispatching an agent.
    pub fn user_source(&self, dispatch: bool) -> TokenSourceLiteral {
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

    /// Credentials for a user who may join but not publish. The server
    /// rejects such a user's microphone publish.
    pub fn unpublishable_user_source(&self) -> TokenSourceLiteral {
        let token = self.token(USER, |token| {
            token.with_grants(VideoGrants { can_publish: Some(false), ..self.grants() })
        });
        literal(self.url.clone(), token)
    }

    /// Joins a participant of kind agent reporting `state`.
    pub async fn join_agent(
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
    pub async fn join_avatar_worker(
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

    pub async fn join_observer(&self) -> Result<(Room, UnboundedReceiver<RoomEvent>)> {
        let token = self.token("observer", |token| token);
        Ok(Room::connect(&self.url, &token, RoomOptions::default()).await?)
    }
}

pub async fn set_agent_state(agent: &Room, state: &str) -> Result<()> {
    agent
        .local_participant()
        .set_attributes(HashMap::from([(ATTRIBUTE_AGENT_STATE.to_owned(), state.to_owned())]))
        .await?;
    Ok(())
}

pub async fn wait_for(
    session: &Session,
    what: &str,
    predicate: impl FnMut(&SessionState) -> bool,
) -> Result<SessionState> {
    let mut state = session.state();
    let reached = timeout(WAIT, state.wait_for(predicate))
        .await
        .map_err(|_| anyhow!("timed out waiting for {what}; last {:?}", current(session)))??;
    Ok(reached.clone())
}

/// What a test expects of the agent state, without the agent's tracks.
#[derive(Clone, Copy, Debug)]
pub enum Expect {
    /// [`AgentState::Joined`] with the agent `AGENT` doing the activity.
    Joined(AgentActivity),
    /// [`AgentFailure::Timeout`], with or without an agent in the room.
    TimedOut,
    /// [`AgentFailure::Left`] by the agent `AGENT`.
    Left,
}

impl Expect {
    pub fn matches(self, state: &AgentState) -> bool {
        match (self, state) {
            (Self::Joined(activity), AgentState::Joined(agent)) => {
                agent.identity().as_str() == AGENT && agent.activity() == activity
            }
            (Self::TimedOut, AgentState::Failed(AgentFailure::Timeout { .. })) => true,
            (Self::Left, AgentState::Failed(AgentFailure::Left { identity, .. })) => {
                identity.as_str() == AGENT
            }
            _ => false,
        }
    }
}

pub async fn wait_for_agent(session: &Session, expect: Expect) -> Result<()> {
    wait_for(session, &format!("{expect:?}"), |s| expect.matches(&s.agent)).await?;
    Ok(())
}

pub async fn eventually(what: &str, mut condition: impl FnMut() -> bool) -> Result<()> {
    timeout(WAIT, async {
        while !condition() {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .map_err(|_| anyhow!("timed out waiting for {what}"))
}

pub fn current(session: &Session) -> SessionState {
    session.state().borrow().clone()
}

/// Options for a session that gets credentials from `token_source` and stays
/// off the machine's audio devices.
pub fn text_options(token_source: TokenSourceLiteral) -> SessionOptions {
    SessionOptions::new_fixed_source(token_source).with_platform_audio(false)
}

/// The machine's audio devices, or `None` when it has no microphone to
/// publish, logging that the test is skipped.
pub fn microphone_probe() -> Option<PlatformAudio> {
    let Ok(probe) = PlatformAudio::new() else {
        log::info!("skipping: this machine has no audio devices");
        return None;
    };
    if probe.recording_devices().next().is_none() {
        log::info!("skipping: this machine has no microphone");
        return None;
    }
    Some(probe)
}
