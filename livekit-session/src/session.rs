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

use std::sync::{Arc, Mutex, TryLockError};
use std::time::Duration;
use std::{fmt, mem};

use livekit::prelude::*;
use livekit::SendTextOptions;
use livekit_token_source::{
    TokenSourceConfigurable, TokenSourceFetchOptions, TokenSourceFixed, TokenSourceResponse,
    TokenSourceResult,
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::actor::{lock, Connection, SessionActor, Shared};
use crate::agent::AgentState;
use crate::constants::{DEFAULT_AGENT_CONNECT_TIMEOUT, TOPIC_CHAT};
use crate::error::{MessageSendError, SessionStartError};
use crate::messages::{Message, MessageOrigin, MessageUpdate};

/// Where a [`Session`] gets its credentials, chosen by the [`SessionOptions`]
/// constructor used.
///
/// The sources are behind an `Arc`, so a clone shares the source instead of
/// copying it: `dyn` types cannot implement `Clone`.
// Built once per session and kept behind an `Arc`, so its size does not matter.
#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum SessionTokenSource {
    /// A source whose credentials take no options.
    Fixed(Arc<dyn TokenSourceFixed + Send + Sync>),
    /// A source fetched with the given options, e.g. to dispatch an agent.
    Configurable(Arc<dyn TokenSourceConfigurable + Send + Sync>, TokenSourceFetchOptions),
}

impl SessionTokenSource {
    async fn fetch(&self) -> TokenSourceResult<TokenSourceResponse> {
        match self {
            Self::Fixed(source) => source.fetch().await,
            Self::Configurable(source, options) => source.fetch(options).await,
        }
    }
}

/// Prints only the kind of source. The source and the fetch options can hold
/// credentials.
impl fmt::Debug for SessionTokenSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fixed(_) => f.write_str("Fixed"),
            Self::Configurable(..) => f.write_str("Configurable"),
        }
    }
}

/// The inputs of a [`Session`]: where it gets its credentials, and how it
/// connects.
///
/// Create it with [`new_configurable_source`](Self::new_configurable_source)
/// or [`new_fixed_source`](Self::new_fixed_source), which select the token
/// source. Then change the defaults through the `with_*` setters.
#[derive(Clone, Debug)]
pub struct SessionOptions {
    source: SessionTokenSource,
    agent_connect_timeout: Duration,
    room_options: RoomOptions,
    platform_audio: bool,
}

impl SessionOptions {
    /// Creates options that fetch credentials from `source` with
    /// `fetch_options`, e.g. to dispatch an agent with
    /// [`TokenSourceFetchOptions::with_agent_name`].
    ///
    /// Every [`Session::start`] fetches anew, so pass the source itself rather
    /// than one wrapped in [`cached`](TokenSourceConfigurable::cached): agents
    /// do not rejoin a room that already existed, and a cached token names the
    /// previous room.
    pub fn new_configurable_source(
        source: impl TokenSourceConfigurable + Send + Sync + 'static,
        fetch_options: TokenSourceFetchOptions,
    ) -> Self {
        Self::from_source(SessionTokenSource::Configurable(Arc::new(source), fetch_options))
    }

    /// Creates options that get credentials from a fixed `source`, such as a
    /// [`literal`](livekit_token_source::literal) one.
    pub fn new_fixed_source(source: impl TokenSourceFixed + Send + Sync + 'static) -> Self {
        Self::from_source(SessionTokenSource::Fixed(Arc::new(source)))
    }

    /// Stands in for `..Default::default()`, which needs a source.
    fn from_source(source: SessionTokenSource) -> Self {
        Self {
            source,
            agent_connect_timeout: DEFAULT_AGENT_CONNECT_TIMEOUT,
            room_options: RoomOptions::default(),
            platform_audio: true,
        }
    }

    /// Sets how long after connecting an agent gets to become conversational
    /// before the agent state reports
    /// [`AgentFailure::Timeout`](crate::AgentFailure::Timeout). Defaults to 20
    /// seconds.
    ///
    /// Armed on every connect, whether or not the token names an agent: an
    /// agent registered without a name is dispatched into every new room.
    pub fn with_agent_connect_timeout(mut self, timeout: Duration) -> Self {
        self.agent_connect_timeout = timeout;
        self
    }

    /// Sets the options for the session's room, including
    /// [`encryption`](RoomOptions::encryption), which can be toggled once
    /// connected through [`Session::room`].
    pub fn with_room_options(mut self, room_options: RoomOptions) -> Self {
        self.room_options = room_options;
        self
    }

    /// Sets whether [`start`](Session::start) uses the platform's audio
    /// devices: the microphone is captured and published as the local
    /// participant's [`TrackSource::Microphone`] track, and the agent's audio
    /// plays through the speakers, until the session ends. Defaults to `true`.
    ///
    /// `start` fails with [`SessionStartError::PlatformAudio`] on a machine
    /// without audio devices. Turn this off to converse by text alone, or to
    /// publish audio of your own through [`Session::room`].
    ///
    /// Device selection and audio processing are shared by every
    /// [`PlatformAudio`] in the process, so configure them on one of your own.
    pub fn with_platform_audio(mut self, platform_audio: bool) -> Self {
        self.platform_audio = platform_audio;
        self
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
/// By default `start` also captures the platform microphone, publishes it as
/// the local participant's microphone track, and plays the agent through the
/// speakers, until the session ends. [`SessionOptions::with_platform_audio`] turns
/// that off.
///
/// Handles are cheap to clone and share one session. Dropping the last handle
/// closes the room.
///
/// ```no_run
/// use livekit_session::{Session, SessionOptions};
/// use livekit_token_source::{development_token_server, TokenSourceFetchOptions};
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let token_source = development_token_server("my-token-server-id");
/// let fetch_options = TokenSourceFetchOptions::new().with_agent_name("my-agent");
/// let session = Session::new(SessionOptions::new_configurable_source(token_source, fetch_options));
///
/// session.start().await?;
/// session.state().wait_for(|state| state.agent.is_connected()).await?;
/// session.send_text("Hello!").await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Session {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    options: SessionOptions,
    pub(crate) shared: Arc<Shared>,
    pub(crate) lifecycle: Mutex<Lifecycle>,
}

/// Prints a snapshot of the session's status. Omits the message contents,
/// the token source and the room options, which can hold user content and
/// credentials.
impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = &self.inner;
        // Never block: the caller can hold the lifecycle lock.
        let lifecycle = match inner.lifecycle.try_lock() {
            Ok(lifecycle) => lifecycle.name(),
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner().name(),
            Err(TryLockError::WouldBlock) => "<locked>",
        };
        let state = inner.shared.watch();
        let state = state.borrow();
        f.debug_struct("Session")
            .field("lifecycle", &format_args!("{lifecycle}"))
            .field("connection", &state.connection)
            .field("agent", &state.agent)
            .field("messages", &inner.shared.message_count())
            .field("agent_connect_timeout", &inner.options.agent_connect_timeout)
            .field("platform_audio", &inner.options.platform_audio)
            .finish_non_exhaustive()
    }
}

pub(crate) enum Lifecycle {
    Idle,
    Starting { generation: u64, cancel: oneshot::Sender<()> },
    Running(Running),
}

pub(crate) struct Running {
    /// The `start()` that spawned the actor.
    generation: u64,
    room: Arc<Room>,
    /// Dropped to stop the actor.
    stop: mpsc::Sender<()>,
    task: JoinHandle<()>,
}

impl Running {
    /// Stops the actor, unless it already stopped on its own, and waits for
    /// it.
    async fn join(self) {
        drop(self.stop);
        if let Err(err) = self.task.await {
            log::error!("session actor failed: {err}");
        }
    }
}

impl Lifecycle {
    /// The name of the variant, for [`Session`]'s `Debug` output.
    fn name(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Starting { .. } => "Starting",
            Self::Running(_) => "Running",
        }
    }

    /// The running room, unless its actor has already stopped on its own.
    fn room(&self) -> Option<&Arc<Room>> {
        match self {
            Self::Running(running) if !running.stop.is_closed() => Some(&running.room),
            _ => None,
        }
    }

    /// Takes the running state of `generation`, leaving the lifecycle idle.
    /// `None` if an `end` or a later `start` already replaced it.
    fn take_running(&mut self, generation: u64) -> Option<Running> {
        match self {
            Self::Running(running) if running.generation == generation => {}
            _ => return None,
        }
        match mem::replace(self, Self::Idle) {
            Self::Running(running) => Some(running),
            _ => None,
        }
    }
}

impl Session {
    /// Creates a session from `options`, which also select its token source.
    /// Nothing happens until [`start`](Self::start).
    pub fn new(options: SessionOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                shared: Arc::new(Shared::new()),
                lifecycle: Mutex::new(Lifecycle::Idle),
            }),
        }
    }

    /// Fetches credentials, connects the room, and publishes the microphone
    /// (see [`SessionOptions::with_platform_audio`]).
    ///
    /// Resolves once the room is connected, not once the agent is: wait for
    /// that on [`state`](Self::state). Fails with
    /// [`AlreadyStarted`](SessionStartError::AlreadyStarted) while the session
    /// is connecting or connected.
    ///
    /// Dropping the future before it resolves leaves the session
    /// disconnected; a room connected by then is closed in the background.
    pub async fn start(&self) -> Result<(), SessionStartError> {
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let (generation, stale) = {
            let mut lifecycle = lock(&self.inner.lifecycle);
            if matches!(*lifecycle, Lifecycle::Starting { .. }) || lifecycle.room().is_some() {
                return Err(SessionStartError::AlreadyStarted);
            }
            let generation = self.inner.shared.begin(SessionState {
                connection: SessionConnectionState::Connecting,
                agent: AgentState::Connecting,
            });
            let starting = Lifecycle::Starting { generation, cancel: cancel_tx };
            let stale = match mem::replace(&mut *lifecycle, starting) {
                Lifecycle::Running(running) => Some(running),
                _ => None,
            };
            (generation, stale)
        };
        let mut guard = StartGuard { inner: &self.inner, generation, armed: true };
        // The actor of a room that disconnected on its own, which has stopped
        // or is about to.
        if let Some(stale) = stale {
            stale.join().await;
        }

        let connection = tokio::select! {
            connection = self.connect() => connection,
            _ = cancel_rx => Err(SessionStartError::Cancelled),
        };
        let connection = connection?;

        // The connected room is handed to its actor before anything else is
        // awaited: from here on, dropping this future stops the actor, which
        // closes the room, rather than dropping a room nobody would close.
        let room = connection.room.clone();
        let (stop_tx, stop_rx) = mpsc::channel(1);
        let (ready_tx, ready_rx) = oneshot::channel();
        let shared = self.inner.shared.clone();
        let actor = SessionActor::new(generation, connection, stop_rx, ready_tx, shared);
        {
            let mut lifecycle = lock(&self.inner.lifecycle);
            let task = tokio::spawn(actor.run());
            if !matches!(*lifecycle, Lifecycle::Starting { generation: g, .. } if g == generation) {
                // `end` was called just as the connection completed. The
                // actor, finding its `stop` sender gone, closes the room.
                drop(stop_tx);
                return Err(SessionStartError::Cancelled);
            }
            *lifecycle = Lifecycle::Running(Running { generation, room, stop: stop_tx, task });
        }

        // The actor publishes the microphone, then answers. Dropping the
        // receiver does not stop it; the guard does.
        let ready = ready_rx.await;
        guard.armed = false;
        match ready {
            Ok(Ok(())) => Ok(()),
            Ok(Err(err)) => {
                // The actor closed the room and is stopping.
                let failed = lock(&self.inner.lifecycle).take_running(generation);
                if let Some(failed) = failed {
                    failed.join().await;
                }
                Err(SessionStartError::Microphone(err))
            }
            // `end` stopped the actor before the microphone was published.
            Err(_) => Err(SessionStartError::Cancelled),
        }
    }

    async fn connect(&self) -> Result<Connection, SessionStartError> {
        let options = &self.inner.options;
        let credentials = options.source.fetch().await?;
        // Platform audio has to exist before the room connects.
        let platform_audio = options.platform_audio.then(PlatformAudio::new).transpose()?;
        let (room, events) = Room::connect(
            &credentials.server_url,
            &credentials.participant_token,
            options.room_options.clone(),
        )
        .await?;
        Ok(Connection {
            room: Arc::new(room),
            events,
            platform_audio,
            agent_timeout: options.agent_connect_timeout,
        })
    }

    /// Closes the room, or cancels a [`start`](Self::start) still in progress.
    ///
    /// Does nothing when there is nothing to end, including after the room
    /// disconnected on its own. A room that fails to close cleanly is logged,
    /// and the session has ended regardless.
    ///
    /// The session can be started again as soon as `end` is called, before
    /// the room has finished closing.
    pub async fn end(&self) {
        let running = {
            let mut lifecycle = lock(&self.inner.lifecycle);
            match mem::replace(&mut *lifecycle, Lifecycle::Idle) {
                Lifecycle::Idle => return,
                Lifecycle::Starting { generation, cancel } => {
                    let _ = cancel.send(());
                    self.inner.shared.publish(generation, SessionState::default());
                    return;
                }
                Lifecycle::Running(running) => running,
            }
        };
        running.join().await;
    }

    /// A receiver of the session's state. [`borrow`](watch::Receiver::borrow)
    /// reads the current value, and [`wait_for`](watch::Receiver::wait_for)
    /// awaits a condition.
    pub fn state(&self) -> watch::Receiver<SessionState> {
        self.inner.shared.watch()
    }

    /// The session's room, while it is connected or reconnecting.
    ///
    /// The room is closed once the session ends, even if the caller still
    /// holds it.
    pub fn room(&self) -> Option<Arc<Room>> {
        lock(&self.inner.lifecycle).room().cloned()
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
    /// earlier run. They keep the order given and are renumbered from zero;
    /// later messages follow them. Of messages sharing an id and participant,
    /// the last wins and takes the place of the first.
    ///
    /// Takes the messages by value; pass `saved.iter().cloned()` to keep
    /// them.
    pub fn restore_messages(&self, messages: impl IntoIterator<Item = Message>) {
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
/// it resolves. An actor already spawned is stopped instead, so that it
/// closes its room and publishes the final state.
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
            self.inner.shared.publish(self.generation, SessionState::default());
            return;
        }
        // Dropped while the actor was publishing the microphone. Dropping its
        // `stop` sender stops it.
        drop(lifecycle.take_running(self.generation));
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
///
/// Holds the agent's tracks (see [`AgentState::audio_track`]), so a value
/// kept after the session moved on keeps those tracks alive.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SessionState {
    /// The state of the session's connection.
    pub connection: SessionConnectionState,
    /// Whether the agent is there, what it is doing, and its tracks.
    pub agent: AgentState,
}

impl Default for SessionState {
    fn default() -> Self {
        Self { connection: SessionConnectionState::Disconnected, agent: AgentState::Disconnected }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use async_trait::async_trait;
    use livekit_token_source::TokenSourceError;

    /// Records the options it is fetched with, then fails.
    struct RecordingSource(Arc<Mutex<Option<TokenSourceFetchOptions>>>);

    #[async_trait]
    impl TokenSourceConfigurable for RecordingSource {
        async fn fetch(
            &self,
            options: &TokenSourceFetchOptions,
        ) -> TokenSourceResult<TokenSourceResponse> {
            *lock(&self.0) = Some(options.clone());
            Err(TokenSourceError::Server { status: 500, body: "test".into() })
        }
    }

    #[tokio::test]
    async fn fetches_with_the_fetch_options() {
        let fetched = Arc::default();
        let source = RecordingSource(Arc::clone(&fetched));
        let fetch_options = TokenSourceFetchOptions::new().with_agent_name("my-agent");
        let options = SessionOptions::new_configurable_source(source, fetch_options)
            .with_platform_audio(false);
        let session = Session::new(options);

        assert!(matches!(session.start().await, Err(SessionStartError::Token(_))));
        let options = lock(&fetched).take().expect("source was not fetched");
        assert_eq!(options, TokenSourceFetchOptions::new().with_agent_name("my-agent"));
    }

    #[test]
    fn debug_prints_status_without_blocking() {
        let source = RecordingSource(Arc::default());
        let fetch_options = TokenSourceFetchOptions::new().with_agent_name("my-agent");
        let options = SessionOptions::new_configurable_source(source, fetch_options)
            .with_platform_audio(false);
        let session = Session::new(options);

        let idle = format!("{session:?}");
        assert!(idle.starts_with(
            "Session { lifecycle: Idle, connection: Disconnected, agent: Disconnected, messages: 0"
        ));
        assert!(!idle.contains("my-agent"));

        let _held = lock(&session.inner.lifecycle);
        assert!(format!("{session:?}").contains("lifecycle: <locked>"));
    }
}
