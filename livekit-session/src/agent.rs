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

use std::collections::HashMap;

use livekit::prelude::*;
use livekit::ParticipantKind;

use crate::constants::{ATTRIBUTE_AGENT_STATE, ATTRIBUTE_PUBLISH_ON_BEHALF};
use crate::session::SessionConnectionState;

/// What the session's agent is doing, projected from the room.
///
/// Most code needs only the predicates: [`AgentState::is_connected`] to show
/// that the agent is there, [`AgentState::is_pending`] to show that it is on
/// its way, and [`AgentState::is_finished`] to show that it is gone or never
/// came.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AgentState {
    /// The session is not connected.
    Disconnected,
    /// The session is connecting, or is connected and no agent has joined yet.
    Connecting,
    /// The agent joined and is starting up.
    Initializing,
    /// The agent joined but is not in a conversation.
    Idle,
    /// The agent is listening to the user.
    Listening,
    /// The agent is preparing a reply.
    Thinking,
    /// The agent is replying.
    Speaking,
    /// The agent did not become conversational in time, or left the room.
    ///
    /// Not final: an agent that joins or becomes conversational afterwards
    /// replaces this state.
    Failed(AgentFailure),
    /// The agent reports an activity this SDK does not recognize.
    Unknown,
}

impl AgentState {
    /// Whether the agent is in the room and in a conversation: listening,
    /// thinking or speaking.
    ///
    /// Stricter than "an agent participant exists": an agent that is still
    /// [`Initializing`](Self::Initializing) is not connected.
    pub fn is_connected(self) -> bool {
        matches!(self, Self::Listening | Self::Thinking | Self::Speaking)
    }

    /// Whether an agent is expected but not yet conversational: connecting,
    /// initializing, or idle.
    pub fn is_pending(self) -> bool {
        matches!(self, Self::Connecting | Self::Initializing | Self::Idle)
    }

    /// Whether the agent is gone or never came: the session is disconnected,
    /// or the agent [`Failed`](Self::Failed).
    ///
    /// A failure can still recover, so this becomes `false` again if an agent
    /// joins later. Call [`Session::end`](crate::Session::end) to give up.
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Disconnected | Self::Failed(_))
    }

    /// Parses the agent's `lk.agent.state` attribute. A missing attribute
    /// means [`Idle`](Self::Idle); an unrecognized one means
    /// [`Unknown`](Self::Unknown).
    fn from_attribute(value: Option<&str>) -> Self {
        match value {
            None => Self::Idle,
            Some("idle") => Self::Idle,
            Some("initializing") => Self::Initializing,
            Some("listening") => Self::Listening,
            Some("thinking") => Self::Thinking,
            Some("speaking") => Self::Speaking,
            Some(_) => Self::Unknown,
        }
    }
}

/// Why an agent is [`AgentState::Failed`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AgentFailure {
    /// No agent became conversational within
    /// [`SessionOptions::agent_connect_timeout`](crate::SessionOptions::agent_connect_timeout).
    Timeout,
    /// The agent left the room while the session stayed connected.
    Left,
}
