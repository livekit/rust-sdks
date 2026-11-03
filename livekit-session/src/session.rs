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

use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::SendTextOptions;
use livekit_token_source::{
    TokenSourceConfigurable, TokenSourceFetchOptions, TokenSourceFixed, TokenSourceResponse,
    TokenSourceResult,
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::actor::{lock, Connection, SessionActor, Shared};
use crate::agent::{self, AgentState};
use crate::constants::{DEFAULT_AGENT_CONNECT_TIMEOUT, TOPIC_CHAT};
use crate::error::{MessageSendError, SessionStartError};
use crate::messages::{Message, MessageOrigin, MessageUpdate};

/// Where a [`Session`] gets its credentials, chosen by the constructor used.
// Built once per session and kept behind an `Arc`, so its size does not matter.
#[allow(clippy::large_enum_variant)]
pub(crate) enum SessionTokenSource {
    /// A source whose credentials take no options.
    Fixed(Box<dyn TokenSourceFixed + Send + Sync>),
    /// A source fetched with the given options, e.g. to dispatch an agent.
    Configurable(Box<dyn TokenSourceConfigurable + Send + Sync>, TokenSourceFetchOptions),
}

impl SessionTokenSource {
    async fn fetch(&self) -> TokenSourceResult<TokenSourceResponse> {
        match self {
            Self::Fixed(source) => source.fetch().await,
            Self::Configurable(source, options) => source.fetch(options).await,
        }
    }
}

/// Options for a [`Session`].
#[derive(Clone, Debug)]
#[non_exhaustive]
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
    /// Whether [`start`](Session::start) uses the platform's audio devices:
    /// the microphone is captured and published as the local participant's
    /// [`TrackSource::Microphone`] track, and the agent's audio plays through
    /// the speakers, until the session ends. Defaults to `true`.
    ///
    /// `start` fails with [`SessionStartError::PlatformAudio`] on a machine
    /// without audio devices. Turn this off to converse by text alone, or to
    /// publish audio of your own through [`Session::room`].
    ///
    /// Device selection and audio processing are shared by every
    /// [`PlatformAudio`] in the process, so configure them on one of your own.
    pub platform_audio: bool,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            agent_connect_timeout: DEFAULT_AGENT_CONNECT_TIMEOUT,
            room_options: RoomOptions::default(),
            platform_audio: true,
        }
    }
}

impl SessionOptions {
    /// Sets [`agent_connect_timeout`](Self::agent_connect_timeout).
    pub fn with_agent_connect_timeout(mut self, timeout: Duration) -> Self {
        self.agent_connect_timeout = timeout;
        self
    }

    /// Sets [`room_options`](Self::room_options).
    pub fn with_room_options(mut self, room_options: RoomOptions) -> Self {
        self.room_options = room_options;
        self
    }

    /// Sets [`platform_audio`](Self::platform_audio).
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
/// speakers, until the session ends. [`SessionOptions::platform_audio`] turns
/// that off.
///
/// Handles are cheap to clone and share one session. Dropping the last handle
/// closes the room.
///
/// ```no_run
/// use livekit_session::{Session, SessionOptions};
/// use livekit_token_source::development_token_server;
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let token_source = development_token_server("my-token-server-id");
/// let session = Session::with_agent(token_source, "my-agent", SessionOptions::default());
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
    token_source: SessionTokenSource,
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
    /// The running room, unless its actor has already stopped on its own.
    fn room(&self) -> Option<&Arc<Room>> {
        match self {
            Self::Running(running) if !running.stop.is_closed() => Some(&running.room),
            _ => None,
        }
    }
}

impl Session {
    /// Creates a session that gets its credentials from a fixed
    /// `token_source`, such as a [`literal`](livekit_token_source::literal)
    /// one. Nothing happens until [`start`](Self::start).
    pub fn new(
        token_source: impl TokenSourceFixed + Send + Sync + 'static,
        session_options: SessionOptions,
    ) -> Self {
        Self::from_source(SessionTokenSource::Fixed(Box::new(token_source)), session_options)
    }

    /// Creates a session that fetches its credentials from `token_source`
    /// with `fetch_options`. Nothing happens until [`start`](Self::start).
    ///
    /// Every `start` fetches anew, so pass the source itself rather than one
    /// wrapped in [`cached`](TokenSourceConfigurable::cached): agents do not
    /// rejoin a room that already existed, and a cached token names the
    /// previous room.
    pub fn with_fetch_options(
        token_source: impl TokenSourceConfigurable + Send + Sync + 'static,
        fetch_options: TokenSourceFetchOptions,
        session_options: SessionOptions,
    ) -> Self {
        Self::from_source(
            SessionTokenSource::Configurable(Box::new(token_source), fetch_options),
            session_options,
        )
    }

    /// Creates a session whose credentials dispatch the agent `agent_name`.
    ///
    /// Shorthand for [`with_fetch_options`](Self::with_fetch_options) with
    /// [`TokenSourceFetchOptions::with_agent_name`]; use that to set further
    /// options, such as agent metadata.
    pub fn with_agent(
        token_source: impl TokenSourceConfigurable + Send + Sync + 'static,
        agent_name: &str,
        session_options: SessionOptions,
    ) -> Self {
        let fetch_options = TokenSourceFetchOptions::new().with_agent_name(agent_name);
        Self::with_fetch_options(token_source, fetch_options, session_options)
    }

    fn from_source(token_source: SessionTokenSource, session_options: SessionOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                token_source,
                options: session_options,
                shared: Arc::new(Shared::new()),
                lifecycle: Mutex::new(Lifecycle::Idle),
            }),
        }
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
#[non_exhaustive]
pub struct SessionState {
    /// The state of the session's connection.
    pub connection: SessionConnectionState,
    /// What the agent is doing.
    pub agent: AgentState,
}

impl Default for SessionState {
    fn default() -> Self {
        Self { connection: SessionConnectionState::Disconnected, agent: AgentState::Disconnected }
    }
}
