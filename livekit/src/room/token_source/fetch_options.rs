// Copyright 2025 LiveKit, Inc.
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
use livekit_protocol::{RoomAgentDispatch, RoomConfiguration, TokenSourceRequest};

/// Options that can be used when fetching new credentials from a TokenSourceConfigurable
///
/// Example:
/// ```rust
/// let _ = TokenSourceFetchOptions::default().with_agent_name("my agent name");
/// ```
#[derive(Clone, Default)]
pub struct TokenSourceFetchOptions {
    pub room_name: Option<String>,
    pub participant_name: Option<String>,
    pub participant_identity: Option<String>,
    pub participant_metadata: Option<String>,
    pub participant_attributes: Option<HashMap<String, String>>,

    pub agent_name: Option<String>,
    pub agent_metadata: Option<String>,
}

impl TokenSourceFetchOptions {
    pub fn with_room_name(mut self, room_name: &str) -> Self {
        self.room_name = Some(room_name.into());
        self
    }
    pub fn with_participant_name(mut self, participant_name: &str) -> Self {
        self.participant_name = Some(participant_name.into());
        self
    }
    pub fn with_participant_identity(mut self, participant_identity: &str) -> Self {
        self.participant_identity = Some(participant_identity.into());
        self
    }
    pub fn with_participant_metadata(mut self, participant_metadata: &str) -> Self {
        self.participant_metadata = Some(participant_metadata.into());
        self
    }

    fn ensure_participant_attributes_defined(&mut self) -> &mut HashMap<String, String> {
        if self.participant_attributes.is_none() {
            self.participant_attributes = Some(HashMap::new());
        };

        let Some(participant_attribute_mut) = self.participant_attributes.as_mut() else { unreachable!(); };
        participant_attribute_mut
    }

    pub fn with_participant_attribute(mut self, attribute_key: &str, attribute_value: &str) -> Self {
        self.ensure_participant_attributes_defined().insert(attribute_key.into(), attribute_value.into());
        self
    }

    pub fn with_participant_attributes(mut self, participant_attributes: HashMap<String, String>) -> Self {
        self.ensure_participant_attributes_defined().extend(participant_attributes);
        self
    }

    pub fn with_agent_name(mut self, agent_name: &str) -> Self {
        self.agent_name = Some(agent_name.into());
        self
    }
    pub fn with_agent_metadata(mut self, agent_metadata: &str) -> Self {
        self.agent_metadata = Some(agent_metadata.into());
        self
    }
}

impl Into<TokenSourceRequest> for TokenSourceFetchOptions {
    fn into(self) -> TokenSourceRequest {
        let mut agent_dispatch = RoomAgentDispatch::default();
        if let Some(agent_name) = self.agent_name {
            agent_dispatch.agent_name = agent_name;
        }
        if let Some(agent_metadata) = self.agent_metadata {
            agent_dispatch.metadata = agent_metadata;
        }

        let room_config = if agent_dispatch != RoomAgentDispatch::default() {
            let mut room_config = RoomConfiguration::default();
            room_config.agents.push(agent_dispatch);
            Some(room_config)
        } else {
            None
        };

        TokenSourceRequest {
            room_name: self.room_name,
            participant_name: self.participant_name,
            participant_identity: self.participant_identity,
            participant_metadata: self.participant_metadata,
            participant_attributes: self.participant_attributes.unwrap_or_default(),
            room_config,
        }
    }
}
