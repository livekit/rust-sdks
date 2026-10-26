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

use crate::agent::AgentState;

/// The connection state of a [`Session`](crate::Session).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// The observable state of a [`Session`](crate::Session) at one point in time.
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
