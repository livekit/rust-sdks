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

/// Per-call overrides used to parameterize a token request.
///
/// Every option is optional: anything left unset is omitted from the request and the
/// server picks a default. Set only the options you care about:
///
/// ```
/// # use livekit_token_source::TokenSourceFetchOptions;
/// let options = TokenSourceFetchOptions::new()
///     .with_room_name("my-room")
///     .with_participant_identity("user-123");
/// ```
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct TokenSourceFetchOptions {
    pub(crate) room_name: Option<String>,
    pub(crate) participant_name: Option<String>,
    pub(crate) participant_identity: Option<String>,
    pub(crate) participant_metadata: Option<String>,
    pub(crate) participant_attributes: Option<HashMap<String, String>>,
    pub(crate) agent_name: Option<String>,
    pub(crate) agent_metadata: Option<String>,
    pub(crate) deployment: Option<String>,
    pub(crate) thread_id: Option<String>,
}

impl TokenSourceFetchOptions {
    /// Creates empty fetch options; the server picks a default for every field.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the name of the room being requested when generating credentials.
    pub fn with_room_name(mut self, value: impl Into<String>) -> Self {
        self.room_name = Some(value.into());
        self
    }

    /// Sets the name of the participant being requested when generating credentials.
    pub fn with_participant_name(mut self, value: impl Into<String>) -> Self {
        self.participant_name = Some(value.into());
        self
    }

    /// Sets the identity of the participant being requested when generating credentials.
    pub fn with_participant_identity(mut self, value: impl Into<String>) -> Self {
        self.participant_identity = Some(value.into());
        self
    }

    /// Sets the metadata of the participant being requested when generating credentials.
    pub fn with_participant_metadata(mut self, value: impl Into<String>) -> Self {
        self.participant_metadata = Some(value.into());
        self
    }

    /// Adds the given attributes to the participant attributes, keeping any set previously.
    /// A key that was already set is overwritten with its new value.
    pub fn with_participant_attributes(
        mut self,
        value: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        self.participant_attributes
            .get_or_insert_with(HashMap::new)
            .extend(value.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }

    /// Adds a single attribute to the participant attributes, keeping any set previously.
    pub fn with_participant_attribute(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.participant_attributes
            .get_or_insert_with(HashMap::new)
            .insert(key.into(), value.into());
        self
    }

    /// Sets the name of the agent to dispatch into the room.
    pub fn with_agent_name(mut self, value: impl Into<String>) -> Self {
        self.agent_name = Some(value.into());
        self
    }

    /// Sets the metadata to pass to the dispatched agent.
    pub fn with_agent_metadata(mut self, value: impl Into<String>) -> Self {
        self.agent_metadata = Some(value.into());
        self
    }

    /// Sets the agent deployment to target. Leave unset to target the production deployment.
    pub fn with_deployment(mut self, value: impl Into<String>) -> Self {
        self.deployment = Some(value.into());
        self
    }

    /// Sets the id of the agent thread that the dispatched agent continues. Leave unset
    /// to start the agent without a thread, so its conversation is not saved.
    pub fn with_thread_id(mut self, value: impl Into<String>) -> Self {
        self.thread_id = Some(value.into());
        self
    }
}

/// The JSON body posted to the token endpoint. Built from [`TokenSourceFetchOptions`];
/// the flat agent fields get nested under `room_config.agents` to match the server's schema.
#[derive(serde::Serialize)]
pub(crate) struct TokenSourceRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    room_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    participant_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    participant_identity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    participant_metadata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    participant_attributes: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    room_config: Option<RoomConfig>,
}

/// Non-exhaustive list of room config parameter, the full list is in livekit_room.proto
#[derive(serde::Serialize)]
struct RoomConfig {
    agents: Vec<AgentDispatch>,
}

#[derive(serde::Serialize)]
struct AgentDispatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deployment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_id: Option<String>,
}

impl From<&TokenSourceFetchOptions> for TokenSourceRequest {
    fn from(options: &TokenSourceFetchOptions) -> TokenSourceRequest {
        // Only include a room_config when at least one agent field is set.
        let room_config = if options.agent_name.is_some()
            || options.agent_metadata.is_some()
            || options.deployment.is_some()
            || options.thread_id.is_some()
        {
            Some(RoomConfig {
                agents: vec![AgentDispatch {
                    agent_name: options.agent_name.clone(),
                    metadata: options.agent_metadata.clone(),
                    deployment: options.deployment.clone(),
                    thread_id: options.thread_id.clone(),
                }],
            })
        } else {
            None
        };

        TokenSourceRequest {
            room_name: options.room_name.clone(),
            participant_name: options.participant_name.clone(),
            participant_identity: options.participant_identity.clone(),
            participant_metadata: options.participant_metadata.clone(),
            participant_attributes: options.participant_attributes.clone(),
            room_config,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(options: &TokenSourceFetchOptions) -> serde_json::Value {
        serde_json::to_value(TokenSourceRequest::from(options)).unwrap()
    }

    #[test]
    fn thread_id_goes_into_the_agent_dispatch() {
        let options =
            TokenSourceFetchOptions::new().with_agent_name("agent").with_thread_id("AT_1");
        assert_eq!(
            body(&options)["room_config"]["agents"],
            serde_json::json!([{ "agent_name": "agent", "thread_id": "AT_1" }])
        );
    }

    #[test]
    fn thread_id_alone_creates_a_dispatch() {
        let options = TokenSourceFetchOptions::new().with_thread_id("AT_1");
        assert_eq!(
            body(&options)["room_config"]["agents"],
            serde_json::json!([{ "thread_id": "AT_1" }])
        );
    }

    #[test]
    fn no_agent_fields_means_no_room_config() {
        assert!(body(&TokenSourceFetchOptions::new()).get("room_config").is_none());
    }
}
