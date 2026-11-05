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

//! Tokens signed locally with the project's API key and secret, for
//! development until a token service supplies them.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use livekit_protocol::{RoomAgentDispatch, RoomConfiguration};
use livekit_token::{AccessToken, AccessTokenError, AgentThreadGrants, VideoGrants};
use livekit_token_source::{
    TokenSourceError, TokenSourceFixed, TokenSourceResponse, TokenSourceResult,
};

use crate::directory::Directory;

/// The node checks the token when a request starts, so it only has to be
/// valid until then.
const THREAD_TOKEN_TTL: Duration = Duration::from_secs(10 * 60);
const ROOM_TOKEN_TTL: Duration = Duration::from_secs(60 * 60);

pub(crate) struct ApiKey {
    api_key: String,
    api_secret: String,
}

impl ApiKey {
    pub(crate) fn new(api_key: &str, api_secret: &str) -> Self {
        Self { api_key: api_key.to_owned(), api_secret: api_secret.to_owned() }
    }

    /// A token for the text endpoint that may read and write `thread_id`, and
    /// nothing else.
    pub(crate) fn thread_token(
        &self,
        scope: &str,
        thread_id: &str,
    ) -> Result<String, AccessTokenError> {
        AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_identity(scope)
            .with_ttl(THREAD_TOKEN_TTL)
            .with_agent_thread_grants(AgentThreadGrants {
                scope: scope.to_owned(),
                thread_ids: vec![thread_id.to_owned()],
                read: true,
                write: true,
                ..Default::default()
            })
            .to_jwt()
    }

    /// A token that joins `room` as `identity` and dispatches the agent of
    /// `dispatch` into it.
    fn room_token(
        &self,
        identity: &str,
        room: &str,
        dispatch: RoomAgentDispatch,
    ) -> Result<String, AccessTokenError> {
        AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_identity(identity)
            .with_ttl(ROOM_TOKEN_TTL)
            .with_grants(VideoGrants {
                room_join: true,
                room: room.to_owned(),
                ..Default::default()
            })
            .with_room_config(RoomConfiguration { agents: vec![dispatch], ..Default::default() })
            .to_jwt()
    }
}

/// The token source of a thread's call. Every fetch signs a token for a new
/// room, because an agent does not rejoin a room that already existed. The
/// dispatch metadata names the thread, so the agent continues it.
pub(crate) struct CallTokenSource {
    directory: Arc<Directory>,
    thread_id: String,
}

impl CallTokenSource {
    pub(crate) fn new(directory: Arc<Directory>, thread_id: &str) -> Self {
        Self { directory, thread_id: thread_id.to_owned() }
    }
}

#[async_trait]
impl TokenSourceFixed for CallTokenSource {
    async fn fetch(&self) -> TokenSourceResult<TokenSourceResponse> {
        let options = &self.directory.options;
        let room = format!("thread-{}-{:08x}", self.thread_id, rand::random::<u32>());
        let dispatch = RoomAgentDispatch {
            agent_name: options.agent_name.clone(),
            metadata: serde_json::json!({ "thread_id": self.thread_id }).to_string(),
            deployment: options.deployment.clone().unwrap_or_default(),
            ..Default::default()
        };
        let participant_token = self
            .directory
            .api_key
            .room_token(&options.scope, &room, dispatch)
            // Signing is local, but the trait's error has no variant for it.
            .map_err(|error| TokenSourceError::Server {
                status: 500,
                body: format!("failed to sign the room token: {error}"),
            })?;
        Ok(TokenSourceResponse { server_url: self.directory.server_url.clone(), participant_token })
    }
}
