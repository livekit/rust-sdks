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

/// A remote participant, as far as agent projection is concerned.
pub(crate) trait AgentCandidate {
    fn identity(&self) -> ParticipantIdentity;
    fn kind(&self) -> ParticipantKind;
    fn attributes(&self) -> HashMap<String, String>;
}

impl AgentCandidate for RemoteParticipant {
    fn identity(&self) -> ParticipantIdentity {
        RemoteParticipant::identity(self)
    }

    fn kind(&self) -> ParticipantKind {
        RemoteParticipant::kind(self)
    }

    fn attributes(&self) -> HashMap<String, String> {
        RemoteParticipant::attributes(self)
    }
}

/// Finds the agent: a participant of kind agent that is not publishing on
/// behalf of another participant (that would be an avatar worker).
///
/// With several agents, the lowest identity wins, so the choice does not
/// change between calls.
pub(crate) fn find_agent<'a, P: AgentCandidate>(
    participants: impl IntoIterator<Item = &'a P>,
) -> Option<&'a P> {
    participants
        .into_iter()
        .filter(|p| {
            p.kind() == ParticipantKind::Agent
                && !p.attributes().contains_key(ATTRIBUTE_PUBLISH_ON_BEHALF)
        })
        .min_by_key(|p| p.identity())
}

/// Finds the avatar worker publishing on behalf of `agent`.
pub(crate) fn find_avatar_worker<'a, P: AgentCandidate>(
    participants: impl IntoIterator<Item = &'a P>,
    agent: &ParticipantIdentity,
) -> Option<&'a P> {
    participants.into_iter().find(|p| {
        p.attributes()
            .get(ATTRIBUTE_PUBLISH_ON_BEHALF)
            .is_some_and(|v| v.as_str() == agent.as_str())
    })
}

/// The agent's microphone track, once subscribed.
pub(crate) fn agent_audio_track<'a>(
    participants: impl IntoIterator<Item = &'a RemoteParticipant>,
) -> Option<RemoteAudioTrack> {
    let agent = find_agent(participants)?;
    agent.track_publications().into_values().find_map(|publication| {
        match (publication.source(), publication.track()) {
            (TrackSource::Microphone, Some(RemoteTrack::Audio(track))) => Some(track),
            _ => None,
        }
    })
}

/// The camera track of the agent's avatar worker, once subscribed.
pub(crate) fn agent_avatar_video_track<'a>(
    participants: impl IntoIterator<Item = &'a RemoteParticipant> + Clone,
) -> Option<RemoteVideoTrack> {
    let agent = find_agent(participants.clone())?;
    let worker = find_avatar_worker(participants, &agent.identity())?;
    worker.track_publications().into_values().find_map(|publication| {
        match (publication.source(), publication.track()) {
            (TrackSource::Camera, Some(RemoteTrack::Video(track))) => Some(track),
            _ => None,
        }
    })
}

/// The activity the agent reports through `lk.agent.state`.
fn activity(agent: &impl AgentCandidate) -> AgentState {
    AgentState::from_attribute(agent.attributes().get(ATTRIBUTE_AGENT_STATE).map(String::as_str))
}
