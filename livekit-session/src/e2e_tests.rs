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
/// How long the client waits for the server to answer a publish before it
/// gives up on it: `TRACK_PUBLISH_TIMEOUT` in `livekit`'s engine.
const PUBLISH_TIMEOUT: Duration = Duration::from_secs(10);

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

    /// The grants of every token: joining the test's room, and setting
    /// attributes.
    fn grants(&self) -> VideoGrants {
        VideoGrants {
            room_join: true,
            room: self.room.clone(),
            can_update_own_metadata: Some(true),
            ..Default::default()
        }
    }

    fn token(&self, identity: &str, customize: impl FnOnce(AccessToken) -> AccessToken) -> String {
        let token = AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_ttl(Duration::from_secs(30 * 60))
            .with_grants(self.grants())
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

    /// Credentials for a user who may join but not publish. The server
    /// answers such a user's microphone publish with nothing, so the publish
    /// stays pending until the client gives up on it.
    fn unpublishable_user_source(&self) -> TokenSourceLiteral {
        let token = self.token(USER, |token| {
            token.with_grants(VideoGrants { can_publish: Some(false), ..self.grants() })
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

/// [`text_options`] for a room that gives up after its first failed join.
fn text_options_without_retries() -> SessionOptions {
    let mut room_options = RoomOptions::default();
    room_options.join_retries = 0;
    text_options().with_room_options(room_options)
}

/// The machine's audio devices, or `None` when it has no microphone to
/// publish, logging that the test is skipped.
fn microphone_probe() -> Option<PlatformAudio> {
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
    let wait =
        tokio::spawn(
            async move { state.wait_for(|s| s.agent.is_connected()).await.map(|s| *s).ok() },
        );
    let (agent, _agent_events) = env.join_agent(AGENT, "listening").await?;
    let connected = timeout(WAIT, wait).await??.ok_or_else(|| anyhow!("state closed"))?;
    assert_eq!(connected.agent, AgentState::Connected(AgentActivity::Listening));

    set_agent_state(&agent, "speaking").await?;
    wait_for_agent(&session, AgentState::Connected(AgentActivity::Speaking)).await?;

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
async fn start_publishes_the_microphone_and_end_releases_the_audio_devices() -> Result<()> {
    // Shares the platform audio handle with the session, to observe the
    // session's reference to it.
    let Some(probe) = microphone_probe() else {
        return Ok(());
    };
    let env = TestEnv::new();
    let session = Session::new(env.user_source(false), SessionOptions::default());

    session.start().await?;
    assert_eq!(probe.ref_count(), 2);
    let room = session.room().ok_or_else(|| anyhow!("no room"))?;
    let publications = room.local_participant().track_publications();
    assert!(publications.values().any(|p| p.source() == TrackSource::Microphone));
    drop(room);

    session.end().await;
    assert_eq!(probe.ref_count(), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_leaving_is_reported_as_left() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(env.user_source(false), text_options());
    session.start().await?;
    let (agent, _agent_events) = env.join_agent(AGENT, "listening").await?;
    wait_for_agent(&session, AgentState::Connected(AgentActivity::Listening)).await?;

    agent.close().await?;
    wait_for_agent(&session, AgentState::Failed(AgentFailure::Left)).await?;
    session.end().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn full_reconnect_does_not_report_left() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(env.user_source(false), text_options());
    session.start().await?;
    let (_agent, _agent_events) = env.join_agent(AGENT, "listening").await?;
    wait_for_agent(&session, AgentState::Connected(AgentActivity::Listening)).await?;
    lock(&session.inner.shared.history).clear();

    let room = session.room().ok_or_else(|| anyhow!("no room"))?;
    room.simulate_scenario(SimulateScenario::FullReconnect).await?;
    eventually("a reconnect and the agent's return", || {
        let history = lock(&session.inner.shared.history);
        history.iter().any(|s| s.connection == SessionConnectionState::Reconnecting)
            && history.last().is_some_and(|s| {
                s.connection == SessionConnectionState::Connected
                    && s.agent == AgentState::Connected(AgentActivity::Listening)
            })
    })
    .await?;

    let history = lock(&session.inner.shared.history).clone();
    assert!(
        !history.iter().any(|s| s.agent == AgentState::Failed(AgentFailure::Left)),
        "a full reconnect was reported as the agent leaving: {history:?}"
    );
    session.end().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn timeout_fires_without_an_agent_and_a_late_agent_recovers() -> Result<()> {
    let env = TestEnv::new();
    let options = text_options().with_agent_connect_timeout(Duration::from_millis(500));
    let session = Session::new(env.user_source(true), options);
    session.start().await?;
    wait_for_agent(&session, AgentState::Failed(AgentFailure::Timeout)).await?;

    let (_agent, _agent_events) = env.join_agent(AGENT, "listening").await?;
    wait_for_agent(&session, AgentState::Connected(AgentActivity::Listening)).await?;
    session.end().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn timeout_is_armed_without_dispatch() -> Result<()> {
    let env = TestEnv::new();
    let options = text_options().with_agent_connect_timeout(Duration::from_millis(500));
    // An agent registered without a name is dispatched automatically, so a
    // token naming none still waits for one.
    let session = Session::new(env.user_source(false), options);
    session.start().await?;
    wait_for_agent(&session, AgentState::Failed(AgentFailure::Timeout)).await?;
    session.end().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn timeout_fires_for_an_agent_stuck_initializing() -> Result<()> {
    let env = TestEnv::new();
    let options = text_options().with_agent_connect_timeout(Duration::from_millis(500));
    // Joined first, so the agent is present before the timeout can fire.
    let (agent, _agent_events) = env.join_agent(AGENT, "initializing").await?;
    let session = Session::new(env.user_source(true), options);
    session.start().await?;
    wait_for_agent(&session, AgentState::Failed(AgentFailure::Timeout)).await?;

    set_agent_state(&agent, "listening").await?;
    wait_for_agent(&session, AgentState::Connected(AgentActivity::Listening)).await?;
    session.end().await;
    Ok(())
}

async fn next_message(
    events: &mut UnboundedReceiver<SessionEvent>,
    mut predicate: impl FnMut(&Message) -> bool,
) -> Result<Message> {
    timeout(WAIT, async {
        while let Some(event) = events.recv().await {
            if let SessionEvent::MessageUpserted(message) = event {
                if predicate(&message) {
                    return Ok(message);
                }
            }
        }
        bail!("session events ended")
    })
    .await?
}

#[tokio::test(flavor = "multi_thread")]
async fn messages_merge_chat_and_transcripts() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(env.user_source(false), text_options());
    let mut events = session.subscribe();
    session.start().await?;
    let (agent, mut agent_events) = env.join_agent(AGENT, "listening").await?;
    wait_for_agent(&session, AgentState::Connected(AgentActivity::Listening)).await?;

    // User input reaches the agent on lk.chat, under the id the feed uses.
    let sent = session.send_text("hello agent").await?;
    assert_eq!((sent.origin, sent.text.as_str()), (MessageOrigin::UserInput, "hello agent"));
    let received = timeout(WAIT, async {
        while let Some(event) = agent_events.recv().await {
            if let RoomEvent::TextStreamOpened { reader, topic, .. } = event {
                if topic == TOPIC_CHAT {
                    let reader = reader.take().ok_or_else(|| anyhow!("reader taken"))?;
                    let id = reader.info().id.clone();
                    return Ok((id, reader.read_all().await?));
                }
            }
        }
        bail!("agent events ended")
    })
    .await??;
    assert_eq!(received, (sent.id.clone(), "hello agent".to_owned()));

    // An agent transcript appends on one stream, finalized by the trailer.
    let mut options = StreamTextOptions::new_with_topic(TOPIC_TRANSCRIPTION);
    options.attributes = HashMap::from([
        (ATTRIBUTE_SEGMENT_ID.to_owned(), "seg-1".to_owned()),
        (ATTRIBUTE_TRANSCRIPTION_FINAL.to_owned(), "false".to_owned()),
    ]);
    let writer = agent.local_participant().stream_text(options).await?;
    writer.write("Hel").await?;
    writer.write("lo").await?;
    let partial = next_message(&mut events, |m| m.id == "seg-1" && m.text == "Hello").await?;
    assert!(!partial.is_final);
    writer
        .close_with_options(
            None,
            Some(HashMap::from([(ATTRIBUTE_TRANSCRIPTION_FINAL.to_owned(), "true".to_owned())])),
        )
        .await?;
    let done = next_message(&mut events, |m| m.id == "seg-1" && m.is_final).await?;
    assert_eq!(done.origin, MessageOrigin::AgentTranscript);
    assert_eq!(done.text, "Hello");
    assert_eq!(done.seq, partial.seq);
    assert_eq!(done.participant_identity.as_str(), AGENT);

    // Chat from the agent reaches the feed too.
    agent
        .local_participant()
        .send_text("hello user", SendTextOptions::new_with_topic(TOPIC_CHAT))
        .await?;
    let chat = next_message(&mut events, |m| m.origin == MessageOrigin::Chat).await?;
    assert_eq!(chat.text, "hello user");

    let feed: Vec<_> = session.messages().into_iter().map(|m| (m.origin, m.text)).collect();
    assert_eq!(
        feed,
        [
            (MessageOrigin::UserInput, "hello agent".to_owned()),
            (MessageOrigin::AgentTranscript, "Hello".to_owned()),
            (MessageOrigin::Chat, "hello user".to_owned()),
        ]
    );

    // A failed send adds nothing.
    session.end().await;
    assert!(matches!(session.send_text("too late").await, Err(MessageSendError::NotConnected)));
    assert_eq!(session.messages().len(), 3);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn streams_on_other_topics_are_left_to_the_app() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(env.user_source(false), text_options());
    session.start().await?;
    let mut app_events = session.room().ok_or_else(|| anyhow!("no room"))?.subscribe();
    let (agent, _agent_events) = env.join_agent(AGENT, "listening").await?;
    wait_for_agent(&session, AgentState::Connected(AgentActivity::Listening)).await?;

    agent
        .local_participant()
        .send_text("for the app", SendTextOptions::new_with_topic("app.custom"))
        .await?;
    let text = timeout(WAIT, async {
        while let Some(event) = app_events.recv().await {
            if let RoomEvent::TextStreamOpened { reader, topic, .. } = event {
                if topic == "app.custom" {
                    let reader = reader.take().ok_or_else(|| anyhow!("session took the reader"))?;
                    return Ok(reader.read_all().await?);
                }
            }
        }
        bail!("app events ended")
    })
    .await??;
    assert_eq!(text, "for the app");
    session.end().await;
    Ok(())
}

// Agent media

/// An avatar worker whose identity sorts before [`AGENT`], so that only its
/// publish-on-behalf attribute tells it apart from the agent.
const AVATAR: &str = "a-avatar";

/// Publishes a silent audio track from `room` as `source`.
async fn publish_audio(room: &Room, source: TrackSource) -> Result<LocalTrackPublication> {
    let rtc_source = NativeAudioSource::new(AudioSourceOptions::default(), 48_000, 1, 100);
    let track = LocalAudioTrack::create_audio_track("audio", RtcAudioSource::Native(rtc_source));
    let options = TrackPublishOptions { source, ..Default::default() };
    Ok(room.local_participant().publish_track(LocalTrack::Audio(track), options).await?)
}

/// Publishes a blank video track from `room` as `source`.
async fn publish_video(room: &Room, source: TrackSource) -> Result<LocalTrackPublication> {
    let rtc_source = NativeVideoSource::new(VideoResolution { width: 16, height: 16 }, false);
    let track = LocalVideoTrack::create_video_track("video", RtcVideoSource::Native(rtc_source));
    let options = TrackPublishOptions { source, ..Default::default() };
    Ok(room.local_participant().publish_track(LocalTrack::Video(track), options).await?)
}

/// Whether the session's room has subscribed to the remote track `sid`.
fn is_subscribed(session: &Session, sid: &TrackSid) -> bool {
    let Some(room) = session.room() else {
        return false;
    };
    room.remote_participants()
        .values()
        .any(|p| p.track_publications().get(sid).is_some_and(|p| p.is_subscribed()))
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_audio_track_is_the_agents_microphone_once_subscribed() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(env.user_source(false), text_options());
    assert!(session.agent_audio_track().is_none());
    session.start().await?;
    let (agent, _agent_events) = env.join_agent(AGENT, "listening").await?;
    wait_for_agent(&session, AgentState::Connected(AgentActivity::Listening)).await?;
    assert!(session.agent_audio_track().is_none());

    // Only the microphone is the agent's voice.
    let other = publish_audio(&agent, TrackSource::ScreenshareAudio).await?;
    eventually("the screen share audio to be subscribed", || is_subscribed(&session, &other.sid()))
        .await?;
    assert!(session.agent_audio_track().is_none());
    assert!(!current(&session).agent_tracks.audio);

    let microphone = publish_audio(&agent, TrackSource::Microphone).await?;
    wait_for(&session, "the agent's audio flag", |s| s.agent_tracks.audio).await?;
    let track = session.agent_audio_track().ok_or_else(|| anyhow!("flag set, track missing"))?;
    assert_eq!(track.sid(), microphone.sid());

    agent.local_participant().unpublish_track(&microphone.sid()).await?;
    wait_for(&session, "the agent's audio flag to clear", |s| !s.agent_tracks.audio).await?;
    assert!(session.agent_audio_track().is_none());

    session.end().await;
    assert!(session.agent_audio_track().is_none());
    assert_eq!(current(&session).agent_tracks, AgentTracks::default());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_avatar_video_track_is_the_avatar_workers_camera_once_subscribed() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(env.user_source(false), text_options());
    session.start().await?;
    let (agent, _agent_events) = env.join_agent(AGENT, "listening").await?;
    wait_for_agent(&session, AgentState::Connected(AgentActivity::Listening)).await?;
    assert!(session.agent_avatar_video_track().is_none());

    // The agent's own camera is not an avatar.
    let agent_camera = publish_video(&agent, TrackSource::Camera).await?;
    eventually("the agent's camera to be subscribed", || {
        is_subscribed(&session, &agent_camera.sid())
    })
    .await?;
    assert!(session.agent_avatar_video_track().is_none());
    assert!(!current(&session).agent_tracks.avatar_video);

    let (avatar, _avatar_events) = env.join_avatar_worker(AVATAR, AGENT).await?;
    let camera = publish_video(&avatar, TrackSource::Camera).await?;
    wait_for(&session, "the avatar's video flag", |s| s.agent_tracks.avatar_video).await?;
    let track =
        session.agent_avatar_video_track().ok_or_else(|| anyhow!("flag set, track missing"))?;
    assert_eq!(track.sid(), camera.sid());
    assert!(session.agent_audio_track().is_none());
    assert!(!current(&session).agent_tracks.audio);

    // The worker is of kind agent too, and reports no agent state. The state
    // still follows the agent, so the worker was not taken for it.
    set_agent_state(&agent, "speaking").await?;
    wait_for_agent(&session, AgentState::Connected(AgentActivity::Speaking)).await?;

    avatar.local_participant().unpublish_track(&camera.sid()).await?;
    wait_for(&session, "the avatar's video flag to clear", |s| !s.agent_tracks.avatar_video)
        .await?;
    assert!(session.agent_avatar_video_track().is_none());

    session.end().await;
    assert!(session.agent_avatar_video_track().is_none());
    assert_eq!(current(&session).agent_tracks, AgentTracks::default());
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

/// Asserts that a start cut short during the microphone publish closed its
/// room, so the observer sees the user leave, and left no actor or task
/// behind.
async fn assert_cut_short_publish_left_nothing(
    session: &Session,
    observer_events: &mut UnboundedReceiver<RoomEvent>,
    baseline_tasks: usize,
) -> Result<()> {
    next_participant_event(observer_events, |event| {
        matches!(event, RoomEvent::ParticipantDisconnected(p) if p.identity().as_str() == USER)
    })
    .await?;
    eventually("the final state", || current(session) == SessionState::default()).await?;
    assert!(is_idle(session));
    eventually("the actor to stop", || weak_shared(session).strong_count() == 1).await?;
    let metrics = tokio::runtime::Handle::current().metrics();
    eventually("tasks to return to baseline", || metrics.num_alive_tasks() <= baseline_tasks).await
}

#[tokio::test(flavor = "multi_thread")]
async fn dropping_a_start_during_the_microphone_publish_closes_the_room() -> Result<()> {
    let Some(_probe) = microphone_probe() else {
        return Ok(());
    };
    let env = TestEnv::new();
    let (_observer, mut observer_events) = env.join_observer().await?;
    let baseline_tasks = tokio::runtime::Handle::current().metrics().num_alive_tasks();
    let session = Session::new(env.unpublishable_user_source(), SessionOptions::default());
    let mut start = Box::pin(session.start());
    // The session has a room once its actor owns it, which is then publishing.
    // The observer must have seen the user join by then, or the server may
    // fold the join and the leave into nothing.
    let publishing = async {
        eventually("the actor to own the room", || session.room().is_some()).await?;
        next_participant_event(&mut observer_events, |event| {
            matches!(event, RoomEvent::ParticipantConnected(p) if p.identity().as_str() == USER)
        })
        .await
    };
    tokio::select! {
        result = &mut start => bail!("start resolved during the publish: {result:?}"),
        publishing = publishing => publishing?,
    }

    drop(start);
    assert_cut_short_publish_left_nothing(&session, &mut observer_events, baseline_tasks).await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_microphone_publish_closes_the_room() -> Result<()> {
    let Some(_probe) = microphone_probe() else {
        return Ok(());
    };
    let env = TestEnv::new();
    let (_observer, mut observer_events) = env.join_observer().await?;
    let baseline_tasks = tokio::runtime::Handle::current().metrics().num_alive_tasks();
    let session = Session::new(env.unpublishable_user_source(), SessionOptions::default());

    // The client gives up on the unanswered publish after its own timeout.
    let result = timeout(PUBLISH_TIMEOUT + WAIT, session.start()).await?;
    assert!(matches!(result, Err(SessionStartError::Microphone(_))), "{result:?}");
    assert_eq!(current(&session), SessionState::default());
    assert!(is_idle(&session));
    assert_cut_short_publish_left_nothing(&session, &mut observer_events, baseline_tasks).await
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
