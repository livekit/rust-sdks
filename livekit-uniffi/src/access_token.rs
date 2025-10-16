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
    AccessToken, AccessTokenError, SIPGrants, TokenVerifier, VideoGrants,
};
use std::{collections::HashMap, time::Duration};
use uniffi::{export, remote, Record};

/// An error that can occur during token generation or verification.
#[remote(Error)]
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
#[remote(Record)]
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
#[remote(Record)]
pub struct SIPGrants {
    pub admin: bool,
    pub call: bool,
}

/// Claims decoded from a valid access token.
#[derive(Record)]
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

    // Fields below have been flattened from the protocol-level `RoomConfiguration` message.
    // Expose more fields as necessary for client use.
    pub room_name: String,
}

/// API credentials for access token generation and verification.
#[derive(Record)]
pub struct ApiCredentials {
    key: String,
    secret: String,
}

/// Options used for generating an access token.
///
/// Any fields left empty will use the token generator's defaults.
///
#[derive(Record)]
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

    // Fields below have been flattened from the protocol-level `RoomConfiguration` message.
    // Expose more fields as necessary for client use.
    #[uniffi(default)]
    room_name: Option<String>,
}

/// Generates an access token.
///
/// If `credentials` are omitted, API key and secret will be read from the environment
/// variables `LIVEKIT_API_KEY` and `LIVEKIT_SECRET` respectively.
///
#[export]
pub fn generate_token(
    options: TokenOptions,
    credentials: Option<ApiCredentials>,
) -> Result<String, AccessTokenError> {
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
    let room_config = livekit_protocol::RoomConfiguration {
        name: options.room_name.unwrap_or_default(),
        ..Default::default()
    };
    Ok(token.with_room_config(room_config).to_jwt()?)
}

/// Verifies an access token.
///
/// If `credentials` are omitted, API key and secret will be read from the environment
/// variables `LIVEKIT_API_KEY` and `LIVEKIT_SECRET` respectively.
///
#[export]
pub fn verify_token(
    token: &str,
    credentials: Option<ApiCredentials>,
) -> Result<Claims, AccessTokenError> {
    let verifier = match credentials {
        Some(credentials) => TokenVerifier::with_api_key(&credentials.key, &credentials.secret),
        None => TokenVerifier::new()?,
    };
    let claims = verifier.verify(token)?;
    let room_name = claims.room_config.map_or(String::default(), |config| config.name);
    Ok(Claims {
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
        room_name,
    })
}
