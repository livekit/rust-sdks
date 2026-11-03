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
