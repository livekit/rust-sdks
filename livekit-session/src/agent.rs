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

/// Projects [`AgentState`] from the room, remembering what the projection
/// needs across calls: whether an agent was ever seen, and whether the
/// agent-connect timeout fired.
#[derive(Debug, Default)]
pub(crate) struct AgentTracker {
    has_ever_connected: bool,
    /// Set when the agent-connect timeout fired, to the agent present at the
    /// time (`None` if there was none).
    timed_out: Option<Option<ParticipantIdentity>>,
}

impl AgentTracker {
    /// Projects the agent state for the given connection state and remote
    /// participants.
    pub(crate) fn project<'a, P: AgentCandidate + 'a>(
        &mut self,
        connection: SessionConnectionState,
        participants: impl IntoIterator<Item = &'a P>,
    ) -> AgentState {
        match connection {
            SessionConnectionState::Disconnected => {
                *self = Self::default();
                return AgentState::Disconnected;
            }
            SessionConnectionState::Connecting => return AgentState::Connecting,
            SessionConnectionState::Connected | SessionConnectionState::Reconnecting => {}
        }

        let Some(agent) = find_agent(participants) else {
            // A full reconnect removes every participant, so an agent missing
            // while reconnecting has not necessarily left.
            if self.has_ever_connected && connection == SessionConnectionState::Connected {
                return AgentState::Failed(AgentFailure::Left);
            }
            if self.timed_out == Some(None) {
                return AgentState::Failed(AgentFailure::Timeout);
            }
            return AgentState::Connecting;
        };

        self.has_ever_connected = true;
        let identity = agent.identity();
        let activity = activity(agent);
        if activity.is_connected() {
            self.timed_out = None;
            return activity;
        }
        // The timeout holds only for the agent that was too slow: one that
        // joins afterwards reports its own activity.
        match &self.timed_out {
            Some(Some(timed_out)) if *timed_out == identity => {
                AgentState::Failed(AgentFailure::Timeout)
            }
            _ => activity,
        }
    }

    /// Records the agent-connect timeout firing. Returns `false` when it does
    /// not count, because the session is not connected or the agent is
    /// already conversational.
    pub(crate) fn time_out<'a, P: AgentCandidate + 'a>(
        &mut self,
        connection: SessionConnectionState,
        participants: impl IntoIterator<Item = &'a P>,
    ) -> bool {
        if !connection.is_connected() {
            return false;
        }
        let agent = find_agent(participants);
        if agent.is_some_and(|agent| activity(agent).is_connected()) {
            return false;
        }
        self.timed_out = Some(agent.map(|agent| agent.identity()));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use SessionConnectionState::{Connected, Connecting, Disconnected, Reconnecting};

    const NO_ONE: &[Fake] = &[];

    #[derive(Clone)]
    struct Fake {
        identity: &'static str,
        kind: ParticipantKind,
        attributes: Vec<(&'static str, &'static str)>,
    }

    impl Fake {
        fn agent(identity: &'static str, state: Option<&'static str>) -> Self {
            let attributes = state.map(|s| vec![(ATTRIBUTE_AGENT_STATE, s)]).unwrap_or_default();
            Self { identity, kind: ParticipantKind::Agent, attributes }
        }

        fn standard(identity: &'static str) -> Self {
            Self { identity, kind: ParticipantKind::Standard, attributes: vec![] }
        }

        fn avatar_for(identity: &'static str, agent: &'static str) -> Self {
            Self {
                identity,
                kind: ParticipantKind::Agent,
                attributes: vec![(ATTRIBUTE_PUBLISH_ON_BEHALF, agent)],
            }
        }
    }

    impl AgentCandidate for Fake {
        fn identity(&self) -> ParticipantIdentity {
            self.identity.to_owned().into()
        }

        fn kind(&self) -> ParticipantKind {
            self.kind
        }

        fn attributes(&self) -> HashMap<String, String> {
            self.attributes.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        }
    }

    fn project(tracker: &mut AgentTracker, participants: &[Fake]) -> AgentState {
        tracker.project(Connected, participants)
    }

    #[test]
    fn attribute_values_map_to_states() {
        let mut tracker = AgentTracker::default();
        for (value, state) in [
            ("initializing", AgentState::Initializing),
            ("idle", AgentState::Idle),
            ("listening", AgentState::Listening),
            ("thinking", AgentState::Thinking),
            ("speaking", AgentState::Speaking),
        ] {
            assert_eq!(project(&mut tracker, &[Fake::agent("agent", Some(value))]), state);
        }
    }

    #[test]
    fn missing_attribute_is_idle_and_unrecognized_is_unknown() {
        let mut tracker = AgentTracker::default();
        assert_eq!(project(&mut tracker, &[Fake::agent("agent", None)]), AgentState::Idle);
        assert_eq!(
            project(&mut tracker, &[Fake::agent("agent", Some("dancing"))]),
            AgentState::Unknown
        );
    }

    #[test]
    fn avatar_worker_is_never_the_agent() {
        let mut tracker = AgentTracker::default();
        let participants = [Fake::avatar_for("avatar", "agent"), Fake::standard("user")];
        assert_eq!(project(&mut tracker, &participants), AgentState::Connecting);

        let participants =
            [Fake::avatar_for("avatar", "agent"), Fake::agent("agent", Some("speaking"))];
        assert_eq!(find_agent(&participants).map(|p| p.identity), Some("agent"));
        assert_eq!(
            find_avatar_worker(&participants, &"agent".to_owned().into()).map(|p| p.identity),
            Some("avatar")
        );
    }

    #[test]
    fn lowest_identity_wins_among_several_agents() {
        let participants = [Fake::agent("b", None), Fake::agent("a", None)];
        assert_eq!(find_agent(&participants).map(|p| p.identity), Some("a"));
    }

    #[test]
    fn session_state_takes_precedence() {
        let mut tracker = AgentTracker::default();
        let participants = [Fake::agent("agent", Some("speaking"))];
        assert_eq!(tracker.project(Disconnected, &participants), AgentState::Disconnected);
        assert_eq!(tracker.project(Connecting, &participants), AgentState::Connecting);
        assert_eq!(tracker.project(Reconnecting, &participants), AgentState::Speaking);
    }

    #[test]
    fn agent_leaving_while_connected_is_left() {
        let mut tracker = AgentTracker::default();
        assert_eq!(project(&mut tracker, NO_ONE), AgentState::Connecting);
        project(&mut tracker, &[Fake::agent("agent", Some("listening"))]);
        assert_eq!(project(&mut tracker, NO_ONE), AgentState::Failed(AgentFailure::Left));
    }

    #[test]
    fn agent_missing_while_reconnecting_is_not_left() {
        let mut tracker = AgentTracker::default();
        project(&mut tracker, &[Fake::agent("agent", Some("listening"))]);
        assert_eq!(tracker.project(Reconnecting, NO_ONE), AgentState::Connecting);
        assert_eq!(
            tracker.project(Reconnecting, &[Fake::agent("agent", Some("listening"))]),
            AgentState::Listening
        );
    }

    #[test]
    fn left_recovers_when_an_agent_returns() {
        let mut tracker = AgentTracker::default();
        project(&mut tracker, &[Fake::agent("agent", Some("listening"))]);
        assert_eq!(project(&mut tracker, NO_ONE), AgentState::Failed(AgentFailure::Left));
        assert_eq!(
            project(&mut tracker, &[Fake::agent("agent-2", Some("initializing"))]),
            AgentState::Initializing
        );
    }

    #[test]
    fn disconnect_resets_has_ever_connected() {
        let mut tracker = AgentTracker::default();
        project(&mut tracker, &[Fake::agent("agent", Some("listening"))]);
        tracker.project(Disconnected, NO_ONE);
        assert_eq!(tracker.project(Connecting, NO_ONE), AgentState::Connecting);
        assert_eq!(project(&mut tracker, NO_ONE), AgentState::Connecting);
    }

    #[test]
    fn timeout_without_agent_fails_until_an_agent_joins() {
        let mut tracker = AgentTracker::default();
        assert!(tracker.time_out(Connected, NO_ONE));
        assert_eq!(project(&mut tracker, NO_ONE), AgentState::Failed(AgentFailure::Timeout));
        assert_eq!(
            project(&mut tracker, &[Fake::agent("agent", Some("initializing"))]),
            AgentState::Initializing
        );
    }

    #[test]
    fn timeout_with_initializing_agent_fails_until_it_is_conversational() {
        let mut tracker = AgentTracker::default();
        let stuck = [Fake::agent("agent", Some("initializing"))];
        assert!(tracker.time_out(Connected, &stuck));
        assert_eq!(project(&mut tracker, &stuck), AgentState::Failed(AgentFailure::Timeout));
        assert_eq!(
            project(&mut tracker, &[Fake::agent("agent", Some("listening"))]),
            AgentState::Listening
        );
        // Recovered for good: falling back to idle is not a timeout.
        assert_eq!(project(&mut tracker, &[Fake::agent("agent", Some("idle"))]), AgentState::Idle);
    }

    #[test]
    fn timeout_does_not_count_once_conversational_or_disconnected() {
        let mut tracker = AgentTracker::default();
        assert!(!tracker.time_out(Connected, &[Fake::agent("agent", Some("speaking"))]));
        assert!(!tracker.time_out(Connecting, NO_ONE));
        assert!(!tracker.time_out(Disconnected, NO_ONE));
        assert_eq!(project(&mut tracker, NO_ONE), AgentState::Connecting);
    }

    #[test]
    fn predicates_match_for_every_state() {
        use AgentState::*;
        let states = [
            Disconnected,
            Connecting,
            Initializing,
            Idle,
            Listening,
            Thinking,
            Speaking,
            Failed(AgentFailure::Timeout),
            Failed(AgentFailure::Left),
            Unknown,
        ];
        for state in states {
            assert_eq!(
                state.is_connected(),
                matches!(state, Listening | Thinking | Speaking),
                "{state:?}"
            );
            assert_eq!(
                state.is_pending(),
                matches!(state, Connecting | Initializing | Idle),
                "{state:?}"
            );
            assert_eq!(state.is_finished(), matches!(state, Disconnected | Failed(_)), "{state:?}");
        }
        assert!(!Initializing.is_connected());
    }
}
