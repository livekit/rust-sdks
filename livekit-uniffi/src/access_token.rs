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

use livekit_api::access_token::{
    self, AccessToken, AccessTokenError, SIPGrants, TokenVerifier, VideoGrants,
};
use livekit_protocol::{self as proto, RoomAgentDispatch};
use std::{collections::HashMap, time::Duration};

/// An error that can occur during token generation or verification.
#[uniffi::remote(Error)]
#[uniffi(flat_error)]
pub enum AccessTokenError {
    InvalidKeys,
    InvalidEnv,
    InvalidClaims,
    Encoding,
}

/// Room permissions
///
/// Maps to the JWT's `video` field.
//
#[uniffi::remote(Record)]
pub struct VideoGrants {
    pub room_create: bool,
    pub room_list: bool,
    pub room_record: bool,
    pub room_admin: bool,
    pub room_join: bool,
    pub room: String,
    pub destination_room: String,
    pub can_publish: bool,
    pub can_subscribe: bool,
    pub can_publish_data: bool,
    pub can_publish_sources: Vec<String>,
    pub can_update_own_metadata: bool,
    pub ingress_admin: bool,
    pub hidden: bool,
    pub recorder: bool,
}

/// SIP grants
///
/// Maps to the JWT's `sip` field.
///
#[uniffi::remote(Record)]
pub struct SIPGrants {
    pub admin: bool,
    pub call: bool,
}

/// Agent dispatch configuration
///
/// Defines which agents should be dispatched to a room.
///
#[uniffi::remote(Record)]
pub struct RoomAgentDispatch {
    pub agent_name: String,
    pub metadata: String,
}

/// Room configuration
///
/// Configuration options for a room.
///
#[derive(uniffi::Record)]
pub struct RoomConfiguration {
    pub name: String,
    pub empty_timeout: u32,
    pub departure_timeout: u32,
    pub max_participants: u32,
    pub metadata: String,
    pub min_playout_delay: u32,
    pub max_playout_delay: u32,
    pub sync_streams: bool,
    pub agents: Vec<RoomAgentDispatch>,
}

impl From<proto::RoomConfiguration> for RoomConfiguration {
    fn from(config: proto::RoomConfiguration) -> Self {
        Self {
            name: config.name,
            empty_timeout: config.empty_timeout,
            departure_timeout: config.departure_timeout,
            max_participants: config.max_participants,
            metadata: config.metadata,
            min_playout_delay: config.min_playout_delay,
            max_playout_delay: config.max_playout_delay,
            sync_streams: config.sync_streams,
            agents: config.agents,
        }
    }
}

/// Claims decoded from a valid access token.
#[derive(uniffi::Record)]
pub struct Claims {
    pub exp: u64,
    pub iss: String,
    pub nbf: u64,
    pub sub: String,
    pub name: String,
    pub video: VideoGrants,
    pub sip: SIPGrants,
    pub sha256: String,
    pub metadata: String,
    pub attributes: HashMap<String, String>,
    pub room_configuration: Option<RoomConfiguration>,
}

impl From<livekit_api::access_token::Claims> for Claims {
    fn from(claims: livekit_api::access_token::Claims) -> Self {
        Self {
            exp: claims.exp as u64,
            iss: claims.iss,
            nbf: claims.nbf as u64,
            sub: claims.sub,
            name: claims.name,
            video: claims.video,
            sip: claims.sip,
            sha256: claims.sha256,
            metadata: claims.metadata,
            attributes: claims.attributes,
            room_configuration: claims.room_config.map(Into::into),
        }
    }
}

/// API credentials for access token generation and verification.
#[derive(uniffi::Record)]
pub struct ApiCredentials {
    key: String,
    secret: String,
}

/// Options used for generating an access token.
///
/// Any fields left empty will use the token generator's defaults.
///
#[derive(uniffi::Record)]
pub struct TokenOptions {
    #[uniffi(default)]
    ttl: Option<Duration>,
    #[uniffi(default)]
    video_grants: Option<VideoGrants>,
    #[uniffi(default)]
    sip_grants: Option<SIPGrants>,
    #[uniffi(default)]
    identity: Option<String>,
    #[uniffi(default)]
    name: Option<String>,
    #[uniffi(default)]
    metadata: Option<String>,
    #[uniffi(default)]
    attributes: Option<HashMap<String, String>>,
    #[uniffi(default)]
    sha256: Option<String>,
    #[uniffi(default)]
    room_configuration: Option<RoomConfiguration>,
}

/// Generates an access token.
///
/// If `credentials` are omitted, API key and secret will be read from the environment
/// variables `LIVEKIT_API_KEY` and `LIVEKIT_SECRET` respectively.
///
#[uniffi::export]
pub fn token_generate(
    options: TokenOptions,
    credentials: Option<ApiCredentials>,
) -> Result<String, AccessTokenError> {
    // TODO: used to test log forwarding, remove
    log::debug!("Generating access token");
    let mut token = match credentials {
        Some(credentials) => AccessToken::with_api_key(&credentials.key, &credentials.secret),
        None => AccessToken::new()?,
    };
    if let Some(ttl) = options.ttl {
        token = token.with_ttl(ttl);
    }
    if let Some(video_grants) = options.video_grants {
        token = token.with_grants(video_grants);
    }
    if let Some(sip_grants) = options.sip_grants {
        token = token.with_sip_grants(sip_grants);
    }
    if let Some(identity) = options.identity {
        token = token.with_identity(&identity);
    }
    if let Some(name) = options.name {
        token = token.with_name(&name);
    }
    if let Some(metadata) = options.metadata {
        token = token.with_metadata(&metadata);
    }
    if let Some(attributes) = options.attributes {
        token = token.with_attributes(&attributes);
    }
    if let Some(sha256) = options.sha256 {
        token = token.with_sha256(&sha256);
    }
    if let Some(room_configuration) = options.room_configuration {
        let room_config = proto::RoomConfiguration {
            name: room_configuration.name,
            empty_timeout: room_configuration.empty_timeout,
            departure_timeout: room_configuration.departure_timeout,
            max_participants: room_configuration.max_participants,
            metadata: room_configuration.metadata,
            min_playout_delay: room_configuration.min_playout_delay,
            max_playout_delay: room_configuration.max_playout_delay,
            sync_streams: room_configuration.sync_streams,
            agents: room_configuration.agents,
            egress: None,
        };
        token = token.with_room_config(room_config);
    }
    Ok(token.to_jwt()?)
}

/// Verifies an access token.
///
/// If `credentials` are omitted, API key and secret will be read from the environment
/// variables `LIVEKIT_API_KEY` and `LIVEKIT_SECRET` respectively.
///
#[uniffi::export]
pub fn token_verify(
    token: &str,
    credentials: Option<ApiCredentials>,
) -> Result<Claims, AccessTokenError> {
    // TODO: used to test log forwarding, remove
    log::debug!("Verifying access token");
    let verifier = match credentials {
        Some(credentials) => TokenVerifier::with_api_key(&credentials.key, &credentials.secret),
        None => TokenVerifier::new()?,
    };
    let claims = verifier.verify(token)?;
    Ok(claims.into())
}

/// Parses an access token without verifying its signature.
///
/// This is useful when you want to inspect token contents without having the secret.
/// The token's expiration (exp) and not-before (nbf) times are still validated.
/// WARNING: Do not use this for authentication - the signature is not verified!
///
#[uniffi::export]
pub fn token_claims_from_unverified(token: &str) -> Result<Claims, AccessTokenError> {
    let claims = access_token::Claims::from_unverified(token)?;
    Ok(claims.into())
}
