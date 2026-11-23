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

//! End-to-end tests against a running `livekit-server --dev` that assert on
//! the crate's internals: the session's lifecycle, its actor, and every state
//! it published. The tests of the public API alone are in
//! `tests/session_test.rs`, which shares the helpers in `tests/common`.
//!
//! Environment variables (the same as `livekit`'s e2e suites):
//! - LIVEKIT_URL (default ws://localhost:7880)
//! - LIVEKIT_API_KEY (default "devkey")
//! - LIVEKIT_API_SECRET (default "secret")
//!
//! Run:
//!   cargo test -p livekit-session --features __lk-e2e-test

use std::sync::{Arc, Weak};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use livekit::prelude::*;
use livekit::SimulateScenario;
use livekit_api::services::room::RoomClient;
use livekit_token_source::{
    literal, TokenSourceConfigurable, TokenSourceError, TokenSourceFetchOptions,
    TokenSourceResponse, TokenSourceResult,
};
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};

use crate::actor::{lock, Shared};
use crate::session::Lifecycle;
use crate::*;
use common::*;

#[path = "../tests/common/mod.rs"]
mod common;

fn weak_shared(session: &Session) -> Weak<Shared> {
    Arc::downgrade(&session.inner.shared)
}

fn is_idle(session: &Session) -> bool {
    matches!(*lock(&session.inner.lifecycle), Lifecycle::Idle)
}

/// [`text_options`] for a room that gives up after its first failed join.
fn text_options_without_retries() -> SessionOptions {
    let mut room_options = RoomOptions::default();
    room_options.join_retries = 0;
    text_options().with_room_options(room_options)
}

#[tokio::test(flavor = "multi_thread")]
async fn start_projects_the_agent_and_end_disconnects() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(env.user_source(false), text_options());
    assert_eq!(current(&session), SessionState::default());
    assert!(session.room().is_none());

    session.start().await?;
    assert_eq!(current(&session).connection, SessionConnectionState::Connected);
    assert_eq!(current(&session).agent, AgentState::Connecting);
    assert!(matches!(session.start().await, Err(SessionStartError::AlreadyStarted)));
    let room = session.room().ok_or_else(|| anyhow!("no room"))?;
    assert!(room.local_participant().track_publications().is_empty());
    drop(room);

    let mut state = session.state();
    let wait = tokio::spawn(async move {
        state.wait_for(|s| s.agent.is_connected()).await.map(|s| s.clone()).ok()
    });
    let (agent, _agent_events) = env.join_agent(AGENT, "listening").await?;
    let connected = timeout(WAIT, wait).await??.ok_or_else(|| anyhow!("state closed"))?;
    assert!(Expect::Joined(AgentActivity::Listening).matches(&connected.agent));

    set_agent_state(&agent, "speaking").await?;
    wait_for_agent(&session, Expect::Joined(AgentActivity::Speaking)).await?;

    session.end().await;
    assert_eq!(current(&session), SessionState::default());
    assert!(session.room().is_none());
    // Ending again is a no-op.
    session.end().await;
    assert_eq!(current(&session), SessionState::default());
    assert!(is_idle(&session));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn full_reconnect_does_not_report_left() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(env.user_source(false), text_options());
    session.start().await?;
    let (_agent, _agent_events) = env.join_agent(AGENT, "listening").await?;
    wait_for_agent(&session, Expect::Joined(AgentActivity::Listening)).await?;
    lock(&session.inner.shared.history).clear();

    let room = session.room().ok_or_else(|| anyhow!("no room"))?;
    room.simulate_scenario(SimulateScenario::FullReconnect).await?;
    eventually("a reconnect and the agent's return", || {
        let history = lock(&session.inner.shared.history);
        history.iter().any(|s| s.connection == SessionConnectionState::Reconnecting)
            && history.last().is_some_and(|s| {
                s.connection == SessionConnectionState::Connected
                    && Expect::Joined(AgentActivity::Listening).matches(&s.agent)
            })
    })
    .await?;

    let history = lock(&session.inner.shared.history).clone();
    assert!(
        !history.iter().any(|s| Expect::Left.matches(&s.agent)),
        "a full reconnect was reported as the agent leaving: {history:?}"
    );
    session.end().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn restarting_after_a_server_side_disconnect() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(env.user_source(false), text_options());
    let rooms =
        RoomClient::with_api_key(&env.url.replacen("ws", "http", 1), &env.api_key, &env.api_secret);
    let disconnect = || async {
        rooms.remove_participant(&env.room, USER).await?;
        wait_for(&session, "disconnect", |s| s.connection == SessionConnectionState::Disconnected)
            .await?;
        anyhow::Ok(())
    };

    session.start().await?;
    let stale_room = Arc::downgrade(&session.room().ok_or_else(|| anyhow!("no room"))?);
    disconnect().await?;
    assert!(session.room().is_none());

    // Restarting joins the stopped actor and releases its room.
    session.start().await?;
    assert!(stale_room.upgrade().is_none(), "the disconnected room is still held");
    disconnect().await?;
    // Ending after a disconnect is a no-op.
    session.end().await;
    assert!(is_idle(&session));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rejected_token_fails_the_start() -> Result<()> {
    let env = TestEnv::new();
    let options = text_options_without_retries();
    let session = Session::new(literal(env.url.clone(), "not a token"), options);
    assert!(matches!(session.start().await, Err(SessionStartError::Connect(_))));
    assert_eq!(current(&session), SessionState::default());
    assert!(is_idle(&session));
    Ok(())
}

/// A source that never answers, so `start` stays connecting.
struct PendingSource;

#[async_trait]
impl TokenSourceConfigurable for PendingSource {
    async fn fetch(&self, _: &TokenSourceFetchOptions) -> TokenSourceResult<TokenSourceResponse> {
        std::future::pending().await
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn end_cancels_a_pending_start() -> Result<()> {
    let session =
        Session::with_fetch_options(PendingSource, TokenSourceFetchOptions::new(), text_options());
    let start = tokio::spawn({
        let session = session.clone();
        async move { session.start().await }
    });
    wait_for(&session, "connecting", |s| s.connection == SessionConnectionState::Connecting)
        .await?;

    session.end().await;
    assert!(matches!(start.await?, Err(SessionStartError::Cancelled)));
    assert_eq!(current(&session), SessionState::default());
    assert!(is_idle(&session));
    Ok(())
}

/// A signal server that accepts one connection and never answers, so
/// `Room::connect` stays in its handshake.
struct SilentServer {
    url: String,
    /// Resolves once the client connected.
    accepted: oneshot::Receiver<()>,
    /// Resolves once the client hung up.
    closed: oneshot::Receiver<()>,
}

impl SilentServer {
    async fn bind() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("ws://{}", listener.local_addr()?);
        let (accepted_tx, accepted) = oneshot::channel();
        let (closed_tx, closed) = oneshot::channel();
        tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let _ = accepted_tx.send(());
            // Reads the handshake request and anything after it, answering
            // nothing, until the client closes the connection.
            let mut buf = [0; 1024];
            while matches!(socket.read(&mut buf).await, Ok(n) if n > 0) {}
            let _ = closed_tx.send(());
        });
        Ok(Self { url, accepted, closed })
    }

    /// A session connecting to this server.
    fn session(&self, env: &TestEnv) -> Session {
        Session::new(literal(self.url.clone(), env.token(USER, |t| t)), Self::session_options())
    }

    /// Options for a session connecting to this server.
    fn session_options() -> SessionOptions {
        let mut room_options = RoomOptions::default();
        room_options.join_retries = 0;
        // Long enough that only a cancel can end the handshake.
        room_options.connect_timeout = Duration::from_secs(60);
        text_options().with_room_options(room_options)
    }
}

/// Asserts that a start cancelled during `Room::connect` hung up on the server,
/// left the session idle without spawning an actor, and leaves no task behind.
async fn assert_cancelled_connect_left_nothing(
    session: &Session,
    closed: oneshot::Receiver<()>,
    baseline_tasks: usize,
) -> Result<()> {
    timeout(WAIT, closed)
        .await
        .map_err(|_| anyhow!("the half-open signal connection was not closed"))??;
    assert_eq!(current(session), SessionState::default());
    assert!(is_idle(session));
    assert_eq!(weak_shared(session).strong_count(), 1, "an actor was spawned");

    let metrics = tokio::runtime::Handle::current().metrics();
    eventually("tasks to return to baseline", || metrics.num_alive_tasks() <= baseline_tasks).await
}

#[tokio::test(flavor = "multi_thread")]
async fn end_cancels_a_start_during_connect() -> Result<()> {
    let env = TestEnv::new();
    let baseline_tasks = tokio::runtime::Handle::current().metrics().num_alive_tasks();
    let server = SilentServer::bind().await?;
    let session = server.session(&env);
    let start = tokio::spawn({
        let session = session.clone();
        async move { session.start().await }
    });
    timeout(WAIT, server.accepted).await??;

    session.end().await;
    assert!(matches!(start.await?, Err(SessionStartError::Cancelled)));
    assert_cancelled_connect_left_nothing(&session, server.closed, baseline_tasks).await
}

#[tokio::test(flavor = "multi_thread")]
async fn dropping_a_start_during_connect_resets_the_session() -> Result<()> {
    let env = TestEnv::new();
    let baseline_tasks = tokio::runtime::Handle::current().metrics().num_alive_tasks();
    let server = SilentServer::bind().await?;
    let session = server.session(&env);
    let mut start = Box::pin(session.start());
    tokio::select! {
        result = &mut start => bail!("start resolved during the handshake: {result:?}"),
        accepted = timeout(WAIT, server.accepted) => accepted??,
    }

    drop(start);
    assert_cancelled_connect_left_nothing(&session, server.closed, baseline_tasks).await
}

#[tokio::test(flavor = "multi_thread")]
async fn dropping_a_pending_start_resets_the_session() -> Result<()> {
    let session =
        Session::with_fetch_options(PendingSource, TokenSourceFetchOptions::new(), text_options());
    assert!(timeout(Duration::from_millis(100), session.start()).await.is_err());
    assert_eq!(current(&session), SessionState::default());
    assert!(is_idle(&session));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_microphone_publish_closes_the_room() -> Result<()> {
    let Some(_probe) = microphone_probe() else {
        return Ok(());
    };
    let env = TestEnv::new();
    // Keeps the room open, so that the server can be asked who is in it.
    let (_observer, _) = env.join_observer().await?;
    let baseline_tasks = tokio::runtime::Handle::current().metrics().num_alive_tasks();
    let session = Session::new(env.unpublishable_user_source(), SessionOptions::default());

    // The server rejects the publish, which fails the start.
    let result = timeout(WAIT, session.start()).await?;
    assert!(matches!(result, Err(SessionStartError::Microphone(_))), "{result:?}");
    assert_eq!(current(&session), SessionState::default());
    assert!(is_idle(&session));
    eventually("the actor to stop", || weak_shared(&session).strong_count() == 1).await?;
    let metrics = tokio::runtime::Handle::current().metrics();
    eventually("tasks to return to baseline", || metrics.num_alive_tasks() <= baseline_tasks)
        .await?;

    // The user joins and leaves so quickly that the server may fold the two
    // into nothing, so the observer cannot wait for the leave. Ask the server
    // instead. This runs after the task check, as the HTTP client can spawn
    // tasks of its own.
    let rooms =
        RoomClient::with_api_key(&env.url.replacen("ws", "http", 1), &env.api_key, &env.api_secret);
    timeout(WAIT, async {
        while rooms.list_participants(&env.room).await?.iter().any(|p| p.identity == USER) {
            sleep(Duration::from_millis(100)).await;
        }
        anyhow::Ok(())
    })
    .await
    .map_err(|_| anyhow!("timed out waiting for the user to leave the room"))?
}

// Lifecycle (AGENTS.md "Memory lifecycle")

#[tokio::test(flavor = "multi_thread")]
async fn dropping_the_session_closes_the_room() -> Result<()> {
    let env = TestEnv::new();
    let (_observer, mut observer_events) = env.join_observer().await?;
    let session = Session::new(env.user_source(false), text_options());
    session.start().await?;
    // Otherwise the server may fold the user's join and leave into nothing.
    next_participant_event(&mut observer_events, |event| {
        matches!(event, RoomEvent::ParticipantConnected(p) if p.identity().as_str() == USER)
    })
    .await?;

    let room = session.room().ok_or_else(|| anyhow!("no room"))?;
    let weak_room = Arc::downgrade(&room);
    let weak_shared = weak_shared(&session);
    drop(room);
    drop(session);

    eventually("the actor to stop", || weak_shared.upgrade().is_none()).await?;
    eventually("the room to be released", || weak_room.upgrade().is_none()).await?;
    next_participant_event(&mut observer_events, |event| {
        matches!(event, RoomEvent::ParticipantDisconnected(p) if p.identity().as_str() == USER)
    })
    .await
}

async fn next_participant_event(
    events: &mut UnboundedReceiver<RoomEvent>,
    mut predicate: impl FnMut(&RoomEvent) -> bool,
) -> Result<()> {
    timeout(WAIT, async {
        while let Some(event) = events.recv().await {
            if predicate(&event) {
                return Ok(());
            }
        }
        bail!("room events ended")
    })
    .await
    .map_err(|_| anyhow!("timed out waiting for a participant event"))?
}

#[tokio::test(flavor = "multi_thread")]
async fn repeated_start_and_end_leave_nothing_behind() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(env.user_source(false), text_options());
    let metrics = tokio::runtime::Handle::current().metrics();
    let settle = || async {
        // Room teardown finishes in tasks of its own after close returns.
        sleep(Duration::from_millis(500)).await;
        metrics.num_alive_tasks()
    };

    session.start().await?;
    session.end().await;
    let after_one = settle().await;
    for _ in 0..3 {
        session.start().await?;
        session.end().await;
    }
    let after_four = settle().await;
    assert!(after_four <= after_one, "tasks grew from {after_one} to {after_four}");
    assert_eq!(session.inner.shared.subscriber_count(), 0);

    let weak_shared = weak_shared(&session);
    assert_eq!(weak_shared.strong_count(), 1, "only the session holds the shared state");
    drop(session);
    assert!(weak_shared.upgrade().is_none());
    Ok(())
}

/// A source that always fails.
struct FailingSource;

#[async_trait]
impl TokenSourceConfigurable for FailingSource {
    async fn fetch(&self, _: &TokenSourceFetchOptions) -> TokenSourceResult<TokenSourceResponse> {
        Err(TokenSourceError::Server { status: 500, body: "test".into() })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_starts_spawn_nothing() -> Result<()> {
    let session =
        Session::with_fetch_options(FailingSource, TokenSourceFetchOptions::new(), text_options());
    for _ in 0..2 {
        assert!(matches!(session.start().await, Err(SessionStartError::Token(_))));
        assert_eq!(current(&session), SessionState::default());
        assert!(is_idle(&session));
    }
    assert_eq!(weak_shared(&session).strong_count(), 1);

    let env = TestEnv::new();
    let options = text_options_without_retries();
    let session = Session::new(literal("ws://127.0.0.1:1", env.token(USER, |t| t)), options);
    assert!(matches!(session.start().await, Err(SessionStartError::Connect(_))));
    assert_eq!(current(&session), SessionState::default());
    assert!(is_idle(&session));
    assert_eq!(weak_shared(&session).strong_count(), 1);
    Ok(())
}
