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

use std::mem;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use livekit::prelude::*;
use livekit::SendTextOptions;
use livekit_token_source::{
    TokenSourceConfigurable, TokenSourceFetchOptions, TokenSourceFixed, TokenSourceLiteral,
    TokenSourceResponse, TokenSourceResult,
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::actor::{lock, Command, Connection, SessionActor, Shared};
use crate::agent::{self, AgentState};
use crate::constants::{DEFAULT_AGENT_CONNECT_TIMEOUT, TOPIC_CHAT};
use crate::error::{MessageSendError, SessionStartError};
use crate::messages::{Message, MessageOrigin, MessageUpdate};

/// Where a [`Session`] gets its credentials.
///
/// Usually created through [`From`]: pass a [`TokenSourceConfigurable`] paired
/// with the options to fetch with, or a [`literal`](livekit_token_source::literal)
/// source directly. Wrap other fixed sources with [`SessionTokenSource::fixed`].
pub struct SessionTokenSource(Source);

// Built once per session and kept behind an `Arc`, so its size does not matter.
#[allow(clippy::large_enum_variant)]
enum Source {
    /// A source whose credentials take no options.
    Fixed(Box<dyn TokenSourceFixed + Send + Sync>),
    /// A source fetched with the given options, e.g. to dispatch an agent
    /// through [`TokenSourceFetchOptions::with_agent_name`].
    Configurable(Box<dyn TokenSourceConfigurable + Send + Sync>, TokenSourceFetchOptions),
}

impl SessionTokenSource {
    /// Wraps a source whose credentials take no options.
    pub fn fixed(source: impl TokenSourceFixed + Send + Sync + 'static) -> Self {
        Self(Source::Fixed(Box::new(source)))
    }

    async fn fetch(&self) -> TokenSourceResult<TokenSourceResponse> {
        match &self.0 {
            Source::Fixed(source) => source.fetch().await,
            Source::Configurable(source, options) => source.fetch(options).await,
        }
    }

    /// Drops cached credentials, so the next start fetches new ones. A fixed
    /// source has nothing to drop.
    pub(crate) async fn invalidate(&self) {
        if let Source::Configurable(source, _) = &self.0 {
            source.invalidate().await;
        }
    }
}

// A blanket `From<T: TokenSourceFixed>` would conflict with the tuple impl
// below, so custom fixed sources go through `SessionTokenSource::fixed`.
impl From<TokenSourceLiteral> for SessionTokenSource {
    fn from(source: TokenSourceLiteral) -> Self {
        Self::fixed(source)
    }
}

impl<T: TokenSourceConfigurable + Send + Sync + 'static> From<(T, TokenSourceFetchOptions)>
    for SessionTokenSource
{
    fn from((source, options): (T, TokenSourceFetchOptions)) -> Self {
        Self(Source::Configurable(Box::new(source), options))
    }
}

/// Options for a [`Session`].
#[derive(Clone, Debug)]
pub struct SessionOptions {
    /// How long after connecting an agent gets to become conversational
    /// before the agent state reports
    /// [`AgentFailure::Timeout`](crate::AgentFailure::Timeout). Defaults to 20
    /// seconds.
    ///
    /// Armed on every connect, whether or not the token names an agent: an
    /// agent registered without a name is dispatched into every new room.
    pub agent_connect_timeout: Duration,
    /// Options for the session's room, including
    /// [`encryption`](RoomOptions::encryption), which can be toggled once
    /// connected through [`Session::room`].
    pub room_options: RoomOptions,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            agent_connect_timeout: DEFAULT_AGENT_CONNECT_TIMEOUT,
            room_options: RoomOptions::default(),
        }
    }
}

/// A change to a [`Session`]'s message feed. State changes are observed
/// through [`Session::state`] instead.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum SessionEvent {
    /// A message was added, or updated in place (a growing transcript).
    MessageUpserted(Message),
    /// The feed was replaced by [`Session::restore_messages`].
    MessagesRestored,
}

/// A conversation with a LiveKit agent.
///
/// [`start`](Self::start) fetches credentials and connects a room; the agent is
/// dispatched by the token, or automatically when it is registered without a
/// name. From then on the session projects what the agent
/// is doing ([`state`](Self::state)) and merges chat and transcripts into one
/// feed ([`messages`](Self::messages), [`subscribe`](Self::subscribe)).
///
/// The session reads every text stream on the `lk.chat` and `lk.transcription`
/// topics itself: an app subscribed to the [`room`](Self::room) sees those
/// streams' [`RoomEvent::TextStreamOpened`] with the reader already taken.
/// Streams on other topics are left to the app.
///
/// The session publishes no media. To talk to the agent, publish a microphone
/// track through [`room`](Self::room).
///
/// Handles are cheap to clone and share one session. Dropping the last handle
/// closes the room.
///
/// ```no_run
/// use livekit_session::{Session, SessionOptions};
/// use livekit_token_source::{development_token_server, TokenSourceFetchOptions};
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let source = development_token_server("my-token-server-id");
/// let options = TokenSourceFetchOptions::new().with_agent_name("my-agent");
/// let session = Session::new((source, options), SessionOptions::default());
///
/// session.start().await?;
/// session.state().wait_for(|state| state.agent_state.is_connected()).await?;
/// session.send_text("Hello!").await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Session {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    source: Arc<SessionTokenSource>,
    options: SessionOptions,
    pub(crate) shared: Arc<Shared>,
    pub(crate) lifecycle: Mutex<Lifecycle>,
}

pub(crate) enum Lifecycle {
    Idle,
    Starting { generation: u64, cancel: oneshot::Sender<()> },
    Running(Running),
}

pub(crate) struct Running {
    room: Arc<Room>,
    commands: mpsc::Sender<Command>,
    task: JoinHandle<()>,
}

impl Lifecycle {
    /// The running room, unless its actor has already stopped on its own.
    fn room(&self) -> Option<&Arc<Room>> {
        match self {
            Self::Running(running) if !running.commands.is_closed() => Some(&running.room),
            _ => None,
        }
    }
}

impl Session {
    /// Creates a session that gets its credentials from `source`. Nothing
    /// happens until [`start`](Self::start).
    pub fn new(source: impl Into<SessionTokenSource>, options: SessionOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                source: Arc::new(source.into()),
                options,
                shared: Arc::new(Shared::new()),
                lifecycle: Mutex::new(Lifecycle::Idle),
            }),
        }
    }

    /// Fetches credentials and connects the room.
    ///
    /// Resolves once the room is connected, not once the agent is: wait for
    /// that on [`state`](Self::state). Fails with
    /// [`AlreadyStarted`](SessionStartError::AlreadyStarted) while the session
    /// is connecting or connected. Dropping the future before it resolves
    /// leaves the session disconnected.
    pub async fn start(&self) -> Result<(), SessionStartError> {
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let generation = {
            let mut lifecycle = lock(&self.inner.lifecycle);
            if matches!(*lifecycle, Lifecycle::Starting { .. }) || lifecycle.room().is_some() {
                return Err(SessionStartError::AlreadyStarted);
            }
            let generation = self.inner.shared.begin(SessionSnapshot {
                connection_state: SessionConnectionState::Connecting,
                agent_state: AgentState::Connecting,
            });
            *lifecycle = Lifecycle::Starting { generation, cancel: cancel_tx };
            generation
        };
        let mut guard = StartGuard { inner: &self.inner, generation, armed: true };

        let connection = tokio::select! {
            connection = self.connect() => connection?,
            _ = cancel_rx => return Err(SessionStartError::Cancelled),
        };
        let room = connection.room.clone();
        let (commands_tx, commands_rx) = mpsc::channel(1);
        let mut actor = SessionActor::new(
            generation,
            connection,
            commands_rx,
            self.inner.shared.clone(),
            self.inner.source.clone(),
        );

        {
            let mut lifecycle = lock(&self.inner.lifecycle);
            if matches!(*lifecycle, Lifecycle::Starting { generation: g, .. } if g == generation) {
                actor.refresh();
                let task = tokio::spawn(actor.run());
                *lifecycle = Lifecycle::Running(Running { room, commands: commands_tx, task });
                guard.armed = false;
                return Ok(());
            }
        }

        // `end` was called just as the connection completed.
        drop(actor);
        if let Err(err) = room.close().await {
            log::warn!("failed to close the room of a cancelled start: {err}");
        }
        Err(SessionStartError::Cancelled)
    }

    async fn connect(&self) -> Result<Connection, SessionStartError> {
        let credentials = self.inner.source.fetch().await?;
        let (room, events) = Room::connect(
            &credentials.server_url,
            &credentials.participant_token,
            self.inner.options.room_options.clone(),
        )
        .await?;
        Ok(Connection {
            room: Arc::new(room),
            events,
            agent_timeout: self.inner.options.agent_connect_timeout,
        })
    }

    /// Closes the room, or cancels a [`start`](Self::start) still in progress.
    ///
    /// Does nothing when there is nothing to end, including after the room
    /// disconnected on its own. A room that fails to close cleanly is logged,
    /// and the session has ended regardless.
    pub async fn end(&self) {
        let running = {
            let mut lifecycle = lock(&self.inner.lifecycle);
            match mem::replace(&mut *lifecycle, Lifecycle::Idle) {
                Lifecycle::Idle => return,
                Lifecycle::Starting { generation, cancel } => {
                    let _ = cancel.send(());
                    self.inner.shared.publish(generation, SessionSnapshot::default());
                    return;
                }
                Lifecycle::Running(running) => running,
            }
        };

        // Fails if the actor already stopped on its own, which still leaves
        // its task to join.
        let _ = running.commands.send(Command::End).await;
        if let Err(err) = running.task.await {
            log::error!("session actor failed: {err}");
        }
    }

    /// A receiver of the session's state, which also lets callers await a
    /// condition with [`watch::Receiver::wait_for`].
    pub fn state(&self) -> watch::Receiver<SessionSnapshot> {
        self.inner.shared.watch()
    }

    /// The session's current state.
    pub fn snapshot(&self) -> SessionSnapshot {
        self.inner.shared.snapshot()
    }

    /// The session's room, while it is connected or reconnecting.
    ///
    /// The room is closed once the session ends, even if the caller still
    /// holds it.
    pub fn room(&self) -> Option<Arc<Room>> {
        lock(&self.inner.lifecycle).room().cloned()
    }

    /// The agent's microphone track, once subscribed.
    pub fn agent_audio_track(&self) -> Option<RemoteAudioTrack> {
        let room = self.room()?;
        agent::agent_audio_track(room.remote_participants().values())
    }

    /// The camera track of the agent's avatar, once subscribed. An avatar is a
    /// separate participant publishing on the agent's behalf.
    pub fn agent_avatar_video_track(&self) -> Option<RemoteVideoTrack> {
        let room = self.room()?;
        agent::agent_avatar_video_track(room.remote_participants().values())
    }

    /// Sends `text` to the agent as chat, and adds it to the feed as
    /// [`MessageOrigin::UserInput`] once sent. A failed send adds nothing.
    pub async fn send_text(&self, text: &str) -> Result<Message, MessageSendError> {
        let room = self.room().ok_or(MessageSendError::NotConnected)?;
        let local_participant = room.local_participant();
        let info =
            local_participant.send_text(text, SendTextOptions::new_with_topic(TOPIC_CHAT)).await?;
        Ok(self.inner.shared.upsert(MessageUpdate {
            id: info.id,
            timestamp: info.timestamp,
            origin: MessageOrigin::UserInput,
            participant_identity: local_participant.identity(),
            text: text.to_owned(),
            is_final: true,
        }))
    }

    /// The message feed, ordered by [`Message::seq`].
    pub fn messages(&self) -> Vec<Message> {
        self.inner.shared.messages()
    }

    /// Replaces the feed with `messages`, e.g. a conversation saved from an
    /// earlier run. They are ordered by timestamp and renumbered from zero;
    /// later messages follow them.
    pub fn restore_messages(&self, messages: &[Message]) {
        self.inner.shared.restore(messages);
    }

    /// A receiver of changes to the message feed.
    ///
    /// Unbounded, like [`Room::subscribe`]: the session must never block on a
    /// slow subscriber, and the feed's rate is bounded by the conversation.
    /// Drop the receiver to unsubscribe; it is removed on the next event.
    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<SessionEvent> {
        self.inner.shared.subscribe()
    }
}

/// Resets the session when a [`Session::start`] fails or is dropped before
/// the room is running.
struct StartGuard<'a> {
    inner: &'a Inner,
    generation: u64,
    armed: bool,
}

impl Drop for StartGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let mut lifecycle = lock(&self.inner.lifecycle);
        if matches!(*lifecycle, Lifecycle::Starting { generation, .. } if generation == self.generation)
        {
            *lifecycle = Lifecycle::Idle;
        }
        self.inner.shared.publish(self.generation, SessionSnapshot::default());
    }
}

/// The connection state of a [`Session`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SessionConnectionState {
    /// Not started, ended, or disconnected.
    Disconnected,
    /// Fetching credentials and connecting the room.
    Connecting,
    /// The room is connected.
    Connected,
    /// The room lost its connection and is reconnecting.
    Reconnecting,
}

impl SessionConnectionState {
    /// Whether a room exists and is connected or recovering its connection.
    pub fn is_connected(self) -> bool {
        matches!(self, Self::Connected | Self::Reconnecting)
    }
}

/// The observable state of a [`Session`] at one point in time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionSnapshot {
    /// The state of the session's connection.
    pub connection_state: SessionConnectionState,
    /// What the agent is doing.
    pub agent_state: AgentState,
}

impl SessionSnapshot {
    /// Whether the session is connected or reconnecting. See
    /// [`SessionConnectionState::is_connected`].
    pub fn is_connected(&self) -> bool {
        self.connection_state.is_connected()
    }
}

impl Default for SessionSnapshot {
    fn default() -> Self {
        Self {
            connection_state: SessionConnectionState::Disconnected,
            agent_state: AgentState::Disconnected,
        }
    }
}
