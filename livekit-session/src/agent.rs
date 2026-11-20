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

/// The session's agent, projected from the room: whether it is there, and
/// what it is doing.
///
/// This enum answers the first question. The [`Agent`] inside
/// [`Joined`](Self::Joined) answers the second, and holds the agent's
/// tracks.
///
/// Most code needs only the methods: [`is_connected`](Self::is_connected) to
/// show that the agent is there, [`is_pending`](Self::is_pending) to show
/// that it is on its way, [`is_finished`](Self::is_finished) to show that it
/// is gone or never came, and [`activity`](Self::activity),
/// [`audio_track`](Self::audio_track) and
/// [`avatar_video_track`](Self::avatar_video_track) to read the agent
/// whichever variant holds it.
///
/// ```no_run
/// # use livekit_session::{AgentActivity, Session};
/// # async fn run(session: Session) -> Result<(), Box<dyn std::error::Error>> {
/// let mut state = session.state();
/// let state = state.wait_for(|state| state.agent.audio_track().is_some()).await?.clone();
/// if state.agent.activity() == Some(AgentActivity::Speaking) {
///     let _audio = state.agent.audio_track();
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AgentState {
    /// The session is not connected.
    Disconnected,
    /// The session is connecting, or is connected and no agent has joined yet.
    Connecting,
    /// An agent joined the room. It is not necessarily in a conversation
    /// yet: see [`is_connected`](Self::is_connected).
    Joined(Agent),
    /// The agent did not become conversational in time, or left the room.
    ///
    /// Not final: an agent that joins or becomes conversational afterwards
    /// replaces this state.
    Failed(AgentFailure),
}

impl AgentState {
    /// Whether the agent is in the room and in a conversation: listening,
    /// thinking or speaking.
    ///
    /// Stricter than [`Joined`](Self::Joined): an agent that joined and is
    /// still [`Initializing`](AgentActivity::Initializing) is not connected.
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Joined(agent) if agent.activity.is_conversational())
    }

    /// Whether an agent is expected but not yet conversational: connecting,
    /// initializing, or idle.
    pub fn is_pending(&self) -> bool {
        match self {
            Self::Connecting => true,
            Self::Joined(agent) => {
                matches!(agent.activity, AgentActivity::Initializing | AgentActivity::Idle)
            }
            _ => false,
        }
    }

    /// Whether the agent is gone or never came: the session is disconnected,
    /// or the agent [`Failed`](Self::Failed).
    ///
    /// A failure can still recover, so this becomes `false` again if an agent
    /// joins later. Call [`Session::end`](crate::Session::end) to give up.
    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Disconnected | Self::Failed(_))
    }

    /// The identity of the agent in the room, if there is one.
    pub fn identity(&self) -> Option<&ParticipantIdentity> {
        self.agent().map(Agent::identity)
    }

    /// What the agent in the room is doing, if there is one. An agent that
    /// [`Failed`](Self::Failed) but is still in the room reports its activity
    /// too.
    pub fn activity(&self) -> Option<AgentActivity> {
        self.agent().map(Agent::activity)
    }

    /// The agent's microphone track, once subscribed.
    pub fn audio_track(&self) -> Option<&RemoteAudioTrack> {
        self.agent()?.audio_track()
    }

    /// The camera track of the agent's avatar, once subscribed.
    pub fn avatar_video_track(&self) -> Option<&RemoteVideoTrack> {
        self.agent()?.avatar_video_track()
    }

    /// The agent in the room, whichever variant holds it.
    fn agent(&self) -> Option<&Agent> {
        match self {
            Self::Joined(agent) => Some(agent),
            Self::Failed(AgentFailure::Timeout { agent }) => agent.as_ref(),
            Self::Failed(AgentFailure::Left { .. }) | Self::Disconnected | Self::Connecting => None,
        }
    }
}

/// An agent in the room at one point in time: who it is, what it is doing,
/// and its tracks.
///
/// A snapshot, like the [`SessionState`](crate::SessionState) it comes from:
/// it does not change when the agent does. A value kept after the session
/// moved on keeps its tracks alive, so drop it when a newer state arrives.
///
/// Two agents are equal when their identity, their activity and the `sid`s
/// of their tracks are.
#[derive(Clone, Debug)]
pub struct Agent {
    identity: ParticipantIdentity,
    activity: AgentActivity,
    audio_track: Option<RemoteAudioTrack>,
    avatar_video_track: Option<RemoteVideoTrack>,
}

impl Agent {
    /// The identity of the agent's participant.
    pub fn identity(&self) -> &ParticipantIdentity {
        &self.identity
    }

    /// What the agent is doing.
    pub fn activity(&self) -> AgentActivity {
        self.activity
    }

    /// The agent's microphone track, once subscribed.
    pub fn audio_track(&self) -> Option<&RemoteAudioTrack> {
        self.audio_track.as_ref()
    }

    /// The camera track of the agent's avatar, once subscribed. An avatar is
    /// a separate participant publishing on the agent's behalf.
    pub fn avatar_video_track(&self) -> Option<&RemoteVideoTrack> {
        self.avatar_video_track.as_ref()
    }

    /// Finds the session's agent among the remote participants (see
    /// [`find_agent`]) and takes its snapshot. This is the only place that
    /// decides which participant is the agent.
    pub(crate) fn find<'a, P: AgentCandidate + 'a>(
        participants: impl IntoIterator<Item = &'a P> + Clone,
    ) -> Option<Self> {
        let agent = find_agent(participants.clone())?;
        let identity = agent.identity();
        let avatar_video_track =
            find_avatar_worker(participants, &identity).and_then(AgentCandidate::camera_track);
        let attributes = agent.attributes();
        let activity = attributes.get(ATTRIBUTE_AGENT_STATE).map(String::as_str);
        Some(Self {
            identity,
            activity: AgentActivity::from_attribute(activity),
            audio_track: agent.microphone_track(),
            avatar_video_track,
        })
    }
}

impl PartialEq for Agent {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
            && self.activity == other.activity
            && self.audio_track.as_ref().map(RemoteAudioTrack::sid)
                == other.audio_track.as_ref().map(RemoteAudioTrack::sid)
            && self.avatar_video_track.as_ref().map(RemoteVideoTrack::sid)
                == other.avatar_video_track.as_ref().map(RemoteVideoTrack::sid)
    }
}

impl Eq for Agent {}

/// What an agent in the room is doing, as reported through its
/// `lk.agent.state` attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AgentActivity {
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
    /// The agent reports an activity this SDK does not recognize.
    Unknown,
}

impl AgentActivity {
    /// Whether the agent is in a conversation: listening, thinking or
    /// speaking.
    pub(crate) fn is_conversational(self) -> bool {
        matches!(self, Self::Listening | Self::Thinking | Self::Speaking)
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
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AgentFailure {
    /// No agent became conversational within
    /// [`SessionOptions::with_agent_connect_timeout`](crate::SessionOptions::with_agent_connect_timeout).
    #[non_exhaustive]
    Timeout {
        /// The agent that was too slow, if one is in the room.
        agent: Option<Agent>,
    },
    /// The agent left the room while the session stayed connected.
    #[non_exhaustive]
    Left {
        /// The identity of the agent that left.
        identity: ParticipantIdentity,
    },
}

/// A remote participant, as far as agent projection is concerned.
pub(crate) trait AgentCandidate {
    fn identity(&self) -> ParticipantIdentity;
    fn kind(&self) -> ParticipantKind;
    fn attributes(&self) -> HashMap<String, String>;
    /// The participant's microphone track, once subscribed.
    fn microphone_track(&self) -> Option<RemoteAudioTrack>;
    /// The participant's camera track, once subscribed.
    fn camera_track(&self) -> Option<RemoteVideoTrack>;
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

    fn microphone_track(&self) -> Option<RemoteAudioTrack> {
        self.track_publications().into_values().find_map(|publication| {
            match (publication.source(), publication.track()) {
                (TrackSource::Microphone, Some(RemoteTrack::Audio(track))) => Some(track),
                _ => None,
            }
        })
    }

    fn camera_track(&self) -> Option<RemoteVideoTrack> {
        self.track_publications().into_values().find_map(|publication| {
            match (publication.source(), publication.track()) {
                (TrackSource::Camera, Some(RemoteTrack::Video(track))) => Some(track),
                _ => None,
            }
        })
    }
}

/// Finds the agent: a participant of kind agent that is not publishing on
/// behalf of another participant (that would be an avatar worker).
///
/// With several agents, the lowest identity wins, so the choice does not
/// change between calls.
fn find_agent<'a, P: AgentCandidate>(
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
fn find_avatar_worker<'a, P: AgentCandidate>(
    participants: impl IntoIterator<Item = &'a P>,
    agent: &ParticipantIdentity,
) -> Option<&'a P> {
    participants.into_iter().find(|p| {
        p.attributes()
            .get(ATTRIBUTE_PUBLISH_ON_BEHALF)
            .is_some_and(|v| v.as_str() == agent.as_str())
    })
}

/// Projects [`AgentState`] from the room, remembering what the projection
/// needs across calls: the last agent seen, and whether the agent-connect
/// timeout fired.
#[derive(Debug, Default)]
pub(crate) struct AgentTracker {
    /// The last agent seen since connecting, reported if it leaves.
    last_agent: Option<ParticipantIdentity>,
    /// Set when the agent-connect timeout fired, to the agent present at the
    /// time (`None` if there was none).
    timed_out: Option<Option<ParticipantIdentity>>,
}

impl AgentTracker {
    /// Projects the agent state for the given connection state and the agent
    /// in the room, as [`Agent::find`] returns it.
    pub(crate) fn project(
        &mut self,
        connection: SessionConnectionState,
        agent: Option<Agent>,
    ) -> AgentState {
        match connection {
            SessionConnectionState::Disconnected => {
                *self = Self::default();
                return AgentState::Disconnected;
            }
            SessionConnectionState::Connecting => return AgentState::Connecting,
            SessionConnectionState::Connected | SessionConnectionState::Reconnecting => {}
        }

        let Some(agent) = agent else {
            // A full reconnect removes every participant, so an agent missing
            // while reconnecting has not necessarily left.
            if let (Some(identity), SessionConnectionState::Connected) =
                (&self.last_agent, connection)
            {
                return AgentState::Failed(AgentFailure::Left { identity: identity.clone() });
            }
            if self.timed_out == Some(None) {
                return AgentState::Failed(AgentFailure::Timeout { agent: None });
            }
            return AgentState::Connecting;
        };

        if self.last_agent.as_ref() != Some(&agent.identity) {
            self.last_agent = Some(agent.identity.clone());
        }
        if agent.activity.is_conversational() {
            self.timed_out = None;
            return AgentState::Joined(agent);
        }
        // The timeout holds only for the agent that was too slow: one that
        // joins afterwards reports its own activity.
        match &self.timed_out {
            Some(Some(timed_out)) if *timed_out == agent.identity => {
                AgentState::Failed(AgentFailure::Timeout { agent: Some(agent) })
            }
            _ => AgentState::Joined(agent),
        }
    }

    /// Records the agent-connect timeout firing. Returns `false` when it does
    /// not count, because the session is not connected or the agent is
    /// already conversational.
    pub(crate) fn time_out(
        &mut self,
        connection: SessionConnectionState,
        agent: Option<&Agent>,
    ) -> bool {
        if !connection.is_connected() {
            return false;
        }
        if agent.is_some_and(|agent| agent.activity.is_conversational()) {
            return false;
        }
        self.timed_out = Some(agent.map(|agent| agent.identity.clone()));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use AgentActivity::{Idle, Initializing, Listening, Speaking, Thinking, Unknown};
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
        fn microphone_track(&self) -> Option<RemoteAudioTrack> {
            None
        }

        fn camera_track(&self) -> Option<RemoteVideoTrack> {
            None
        }

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
        project_in(tracker, Connected, participants)
    }

    fn project_in(
        tracker: &mut AgentTracker,
        connection: SessionConnectionState,
        participants: &[Fake],
    ) -> AgentState {
        tracker.project(connection, Agent::find(participants))
    }

    fn time_out(
        tracker: &mut AgentTracker,
        connection: SessionConnectionState,
        participants: &[Fake],
    ) -> bool {
        tracker.time_out(connection, Agent::find(participants).as_ref())
    }

    fn agent(identity: &str, activity: AgentActivity) -> Agent {
        Agent {
            identity: identity.to_owned().into(),
            activity,
            audio_track: None,
            avatar_video_track: None,
        }
    }

    /// The agent `"agent"` joined, doing `activity`.
    fn connected(activity: AgentActivity) -> AgentState {
        AgentState::Joined(agent("agent", activity))
    }

    fn timed_out(agent: Option<Agent>) -> AgentState {
        AgentState::Failed(AgentFailure::Timeout { agent })
    }

    /// The agent `identity` left.
    fn left(identity: &str) -> AgentState {
        AgentState::Failed(AgentFailure::Left { identity: identity.to_owned().into() })
    }

    #[test]
    fn attribute_values_map_to_states() {
        let mut tracker = AgentTracker::default();
        for (value, state) in [
            ("initializing", connected(Initializing)),
            ("idle", connected(Idle)),
            ("listening", connected(Listening)),
            ("thinking", connected(Thinking)),
            ("speaking", connected(Speaking)),
        ] {
            assert_eq!(project(&mut tracker, &[Fake::agent("agent", Some(value))]), state);
        }
    }

    #[test]
    fn missing_attribute_is_idle_and_unrecognized_is_unknown() {
        let mut tracker = AgentTracker::default();
        assert_eq!(project(&mut tracker, &[Fake::agent("agent", None)]), connected(Idle));
        assert_eq!(
            project(&mut tracker, &[Fake::agent("agent", Some("dancing"))]),
            connected(Unknown)
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
        assert_eq!(project_in(&mut tracker, Disconnected, &participants), AgentState::Disconnected);
        assert_eq!(project_in(&mut tracker, Connecting, &participants), AgentState::Connecting);
        assert_eq!(project_in(&mut tracker, Reconnecting, &participants), connected(Speaking));
    }

    #[test]
    fn agent_leaving_while_connected_is_left() {
        let mut tracker = AgentTracker::default();
        assert_eq!(project(&mut tracker, NO_ONE), AgentState::Connecting);
        project(&mut tracker, &[Fake::agent("agent", Some("listening"))]);
        assert_eq!(project(&mut tracker, NO_ONE), left("agent"));
    }

    #[test]
    fn left_names_the_last_agent_seen() {
        let mut tracker = AgentTracker::default();
        project(&mut tracker, &[Fake::agent("b", Some("listening"))]);
        project(&mut tracker, &[Fake::agent("a", Some("listening"))]);
        assert_eq!(project(&mut tracker, NO_ONE), left("a"));
    }

    #[test]
    fn agent_missing_while_reconnecting_is_not_left() {
        let mut tracker = AgentTracker::default();
        project(&mut tracker, &[Fake::agent("agent", Some("listening"))]);
        assert_eq!(project_in(&mut tracker, Reconnecting, NO_ONE), AgentState::Connecting);
        assert_eq!(
            project_in(&mut tracker, Reconnecting, &[Fake::agent("agent", Some("listening"))]),
            connected(Listening)
        );
    }

    #[test]
    fn left_recovers_when_an_agent_returns() {
        let mut tracker = AgentTracker::default();
        project(&mut tracker, &[Fake::agent("agent", Some("listening"))]);
        assert_eq!(project(&mut tracker, NO_ONE), left("agent"));
        assert_eq!(
            project(&mut tracker, &[Fake::agent("agent-2", Some("initializing"))]),
            AgentState::Joined(agent("agent-2", Initializing))
        );
    }

    #[test]
    fn disconnect_forgets_the_last_agent() {
        let mut tracker = AgentTracker::default();
        project(&mut tracker, &[Fake::agent("agent", Some("listening"))]);
        project_in(&mut tracker, Disconnected, NO_ONE);
        assert_eq!(project_in(&mut tracker, Connecting, NO_ONE), AgentState::Connecting);
        assert_eq!(project(&mut tracker, NO_ONE), AgentState::Connecting);
    }

    #[test]
    fn timeout_without_agent_fails_until_an_agent_joins() {
        let mut tracker = AgentTracker::default();
        assert!(time_out(&mut tracker, Connected, NO_ONE));
        assert_eq!(project(&mut tracker, NO_ONE), timed_out(None));
        assert_eq!(
            project(&mut tracker, &[Fake::agent("agent", Some("initializing"))]),
            connected(Initializing)
        );
    }

    #[test]
    fn timeout_with_initializing_agent_fails_until_it_is_conversational() {
        let mut tracker = AgentTracker::default();
        let stuck = [Fake::agent("agent", Some("initializing"))];
        assert!(time_out(&mut tracker, Connected, &stuck));
        // The agent that was too slow stays reachable.
        assert_eq!(project(&mut tracker, &stuck), timed_out(Some(agent("agent", Initializing))));
        assert_eq!(
            project(&mut tracker, &[Fake::agent("agent", Some("listening"))]),
            connected(Listening)
        );
        // Recovered for good: falling back to idle is not a timeout.
        assert_eq!(project(&mut tracker, &[Fake::agent("agent", Some("idle"))]), connected(Idle));
    }

    #[test]
    fn timeout_does_not_count_once_conversational_or_disconnected() {
        let mut tracker = AgentTracker::default();
        assert!(!time_out(&mut tracker, Connected, &[Fake::agent("agent", Some("speaking"))]));
        assert!(!time_out(&mut tracker, Connecting, NO_ONE));
        assert!(!time_out(&mut tracker, Disconnected, NO_ONE));
        assert_eq!(project(&mut tracker, NO_ONE), AgentState::Connecting);
    }

    #[test]
    fn predicates_match_for_every_state() {
        let states = [
            AgentState::Disconnected,
            AgentState::Connecting,
            connected(Initializing),
            connected(Idle),
            connected(Listening),
            connected(Thinking),
            connected(Speaking),
            connected(Unknown),
            timed_out(None),
            timed_out(Some(agent("agent", Initializing))),
            left("agent"),
        ];
        for state in &states {
            let activity = state.activity();
            assert_eq!(
                state.is_connected(),
                matches!(state, AgentState::Joined(_))
                    && matches!(activity, Some(Listening | Thinking | Speaking)),
                "{state:?}"
            );
            assert_eq!(
                state.is_pending(),
                matches!(state, AgentState::Connecting)
                    || matches!(state, AgentState::Joined(_))
                        && matches!(activity, Some(Initializing | Idle)),
                "{state:?}"
            );
            assert_eq!(
                state.is_finished(),
                matches!(state, AgentState::Disconnected | AgentState::Failed(_)),
                "{state:?}"
            );
        }
        assert!(!connected(Initializing).is_connected());
        assert!(!connected(Unknown).is_pending());
    }

    #[test]
    fn accessors_read_the_agent_from_either_variant() {
        let stuck = timed_out(Some(agent("agent", Initializing)));
        assert_eq!(stuck.identity().map(ParticipantIdentity::as_str), Some("agent"));
        assert_eq!(stuck.activity(), Some(Initializing));
        assert_eq!(connected(Speaking).activity(), Some(Speaking));
        for state in [AgentState::Disconnected, AgentState::Connecting] {
            assert_eq!(state.identity(), None);
            assert_eq!(state.activity(), None);
            assert!(state.audio_track().is_none());
            assert!(state.avatar_video_track().is_none());
        }
    }
}
