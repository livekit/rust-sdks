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

use livekit_protocol as proto;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct TokenSourceRequest {
    /// The name of the room being requested when generating credentials
    pub room_name: Option<String>,

    /// The name of the participant being requested for this client when generating credentials
    pub participant_name: Option<String>,

    /// The identity of the participant being requested for this client when generating credentials
    pub participant_identity: Option<String>,

    /// Any participant metadata being included along with the credentials generation operation
    pub participant_metadata: Option<String>,

    /// Any participant attributes being included along with the credentials generation operation
    pub participant_attributes: HashMap<String, String>,

    /// A RoomConfiguration object can be passed to request extra parameters should be included when
    /// generating connection credentials - dispatching agents, defining egress settings, etc
    /// More info: <https://docs.livekit.io/home/get-started/authentication/#room-configuration>
    pub room_config: Option<proto::RoomConfiguration>,
}

impl From<TokenSourceRequest> for proto::TokenSourceRequest {
    fn from(value: TokenSourceRequest) -> Self {
        proto::TokenSourceRequest {
            room_name: value.room_name,
            participant_name: value.participant_name,
            participant_identity: value.participant_identity,
            participant_metadata: value.participant_metadata,
            participant_attributes: value.participant_attributes,
            room_config: value.room_config,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenSourceResponse {
    pub server_url: String,
    pub participant_token: String,
}

impl From<proto::TokenSourceResponse> for TokenSourceResponse {
    fn from(value: proto::TokenSourceResponse) -> Self {
        TokenSourceResponse {
            server_url: value.server_url,
            participant_token: value.participant_token,
        }
    }
}
