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

use std::{
    collections::HashMap,
    env,
    fmt::Debug,
    ops::Add,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use jsonwebtoken::{self, DecodingKey, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::get_env_keys;

pub const DEFAULT_TTL: Duration = Duration::from_secs(3600 * 6); // 6 hours

#[derive(Debug, Error)]
pub enum AccessTokenError {
    #[error("Invalid API Key or Secret Key")]
    InvalidKeys,
    #[error("Invalid environment")]
    InvalidEnv(#[from] env::VarError),
    #[error("invalid claims: {0}")]
    InvalidClaims(&'static str),
    #[error("failed to encode jwt")]
    Encoding(#[from] jsonwebtoken::errors::Error),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct VideoGrants {
    // actions on rooms
    #[serde(skip_serializing_if = "is_default")]
    pub room_create: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub room_list: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub room_record: bool,

    // actions on a particular room
    #[serde(skip_serializing_if = "is_default")]
    pub room_admin: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub room_join: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub room: String,
    #[serde(skip_serializing_if = "is_default")]
    pub destination_room: String,

    // permissions within a room
    #[serde(skip_serializing_if = "is_default")]
    pub can_publish: Option<bool>,
    #[serde(skip_serializing_if = "is_default")]
    pub can_subscribe: Option<bool>,
    #[serde(skip_serializing_if = "is_default")]
    pub can_publish_data: Option<bool>,

    // TrackSource types that a participant may publish.
    // When set, it supercedes CanPublish. Only sources explicitly set here can be published
    #[serde(skip_serializing_if = "is_default")]
    pub can_publish_sources: Vec<String>, // keys keep track of each source

    // by default, a participant is not allowed to update its own metadata
    #[serde(skip_serializing_if = "is_default")]
    pub can_update_own_metadata: Option<bool>,

    // actions on ingresses
    #[serde(skip_serializing_if = "is_default")]
    pub ingress_admin: bool, // applies to all ingress

    // participant is not visible to other participants (useful when making bots)
    #[serde(skip_serializing_if = "is_default")]
    pub hidden: bool,

    // indicates to the room that current participant is a recorder
    #[serde(skip_serializing_if = "is_default")]
    pub recorder: bool,

    // indicates to the room that current participant is an agent
    #[serde(skip_serializing_if = "is_default")]
    pub agent: bool,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

impl VideoGrants {
    pub fn can_publish(&self) -> bool {
        self.can_publish.unwrap_or(true)
    }

    pub fn can_subscribe(&self) -> bool {
        self.can_subscribe.unwrap_or(true)
    }

    pub fn can_publish_data(&self) -> bool {
        self.can_publish_data.unwrap_or_else(|| self.can_publish())
    }

    pub fn can_update_own_metadata(&self) -> bool {
        self.can_update_own_metadata.unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SIPGrants {
    // manage sip resources
    #[serde(default, skip_serializing_if = "is_default")]
    pub admin: bool,
    // make outbound calls
    #[serde(default, skip_serializing_if = "is_default")]
    pub call: bool,
}

impl Default for SIPGrants {
    fn default() -> Self {
        Self { admin: false, call: false }
    }
}

#[derive(Debug, Clone, Serialize, Default, Deserialize, PartialEq)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
pub struct Claims {
    pub exp: usize,  // Expiration
    pub iss: String, // ApiKey
    pub nbf: usize,
    #[serde(skip_serializing_if = "is_default")]
    pub sub: String, // Identity

    #[serde(skip_serializing_if = "is_default")]
    pub name: String,
    #[serde(skip_serializing_if = "is_default")]
    pub kind: String,
    #[serde(skip_serializing_if = "is_default")]
    pub video: VideoGrants,
    #[serde(skip_serializing_if = "is_default")]
    pub sip: SIPGrants,
    #[serde(skip_serializing_if = "is_default")]
    pub sha256: String, // Used to verify the integrity of the message body
    #[serde(skip_serializing_if = "is_default")]
    pub metadata: String,
    #[serde(skip_serializing_if = "is_default")]
    pub attributes: HashMap<String, String>,
    #[serde(skip_serializing_if = "is_default")]
    pub room_config: Option<livekit_protocol::RoomConfiguration>,
}

impl Claims {
    pub fn from_unverified(token: &str) -> Result<Self, AccessTokenError> {
        crate::jwt_provider::ensure_installed();
        let token = jsonwebtoken::dangerous::insecure_decode::<Claims>(token)?;
        Ok(token.claims)
    }
}

#[derive(Clone)]
pub struct AccessToken {
    api_key: String,
    api_secret: String,
    claims: Claims,
}

impl Debug for AccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Don't show api_secret here
        f.debug_struct("AccessToken")
            .field("api_key", &self.api_key)
            .field("claims", &self.claims)
            .finish()
    }
}

impl AccessToken {
    pub fn with_api_key(api_key: &str, api_secret: &str) -> Self {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        Self {
            api_key: api_key.to_owned(),
            api_secret: api_secret.to_owned(),
            claims: Claims {
                exp: now.add(DEFAULT_TTL).as_secs() as usize,
                iss: api_key.to_owned(),
                nbf: now.as_secs() as usize,
                sub: Default::default(),
                name: Default::default(),
                kind: Default::default(),
                video: VideoGrants::default(),
                sip: SIPGrants::default(),
                sha256: Default::default(),
                metadata: Default::default(),
                attributes: HashMap::new(),
                room_config: Default::default(),
            },
        }
    }

    #[cfg(test)]
    pub fn from_parts(api_key: &str, api_secret: &str, claims: Claims) -> Self {
        Self { api_key: api_key.to_owned(), api_secret: api_secret.to_owned(), claims }
    }

    pub fn new() -> Result<Self, AccessTokenError> {
        // Try to get the API Key and the Secret Key from the environment
        let (api_key, api_secret) = get_env_keys()?;
        Ok(Self::with_api_key(&api_key, &api_secret))
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap() + ttl;
        self.claims.exp = time.as_secs() as usize;
        self
    }

    pub fn with_grants(mut self, grants: VideoGrants) -> Self {
        self.claims.video = grants;
        self
    }

    pub fn with_sip_grants(mut self, grants: SIPGrants) -> Self {
        self.claims.sip = grants;
        self
    }

    pub fn with_identity(mut self, identity: &str) -> Self {
        self.claims.sub = identity.to_owned();
        self
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.claims.name = name.to_owned();
        self
    }

    pub fn with_kind(mut self, kind: &str) -> Self {
        self.claims.kind = kind.to_owned();
        self
    }

    pub fn with_metadata(mut self, metadata: &str) -> Self {
        self.claims.metadata = metadata.to_owned();
        self
    }

    pub fn with_attributes<I, K, V>(mut self, attributes: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.claims.attributes =
            attributes.into_iter().map(|(k, v)| (k.into(), v.into())).collect::<HashMap<_, _>>();
        self
    }

    pub fn with_sha256(mut self, sha256: &str) -> Self {
        self.claims.sha256 = sha256.to_owned();
        self
    }

    pub fn with_room_config(mut self, config: livekit_protocol::RoomConfiguration) -> Self {
        self.claims.room_config = Some(config);
        self
    }

    pub fn to_jwt(self) -> Result<String, AccessTokenError> {
        crate::jwt_provider::ensure_installed();
        if self.api_key.is_empty() || self.api_secret.is_empty() {
            return Err(AccessTokenError::InvalidKeys);
        }

        if self.claims.video.room_join
            && (self.claims.sub.is_empty() || self.claims.video.room.is_empty())
        {
            return Err(AccessTokenError::InvalidClaims(
                "token grants room_join but doesn't have an identity or room",
            ));
        }

        Ok(jsonwebtoken::encode(
            &Header::new(jsonwebtoken::Algorithm::HS256),
            &self.claims,
            &EncodingKey::from_secret(self.api_secret.as_ref()),
        )?)
    }
}

#[derive(Clone)]
pub struct TokenVerifier {
    api_key: String,
    api_secret: String,
}

impl Debug for TokenVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenVerifier").field("api_key", &self.api_key).finish()
    }
}

impl TokenVerifier {
    pub fn with_api_key(api_key: &str, api_secret: &str) -> Self {
        Self { api_key: api_key.to_owned(), api_secret: api_secret.to_owned() }
    }

    pub fn new() -> Result<Self, AccessTokenError> {
        let (api_key, api_secret) = get_env_keys()?;
        Ok(Self::with_api_key(&api_key, &api_secret))
    }

    pub fn verify(&self, token: &str) -> Result<Claims, AccessTokenError> {
        crate::jwt_provider::ensure_installed();
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.validate_exp = true;
        validation.validate_nbf = true;
        validation.set_issuer(&[&self.api_key]);

        let token = jsonwebtoken::decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.api_secret.as_ref()),
            &validation,
        )?;

        Ok(token.claims)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{AccessToken, Claims, SIPGrants, TokenVerifier, VideoGrants};

    const TEST_API_KEY: &str = "myapikey";
    const TEST_API_SECRET: &str = "thiskeyistotallyunsafe";
    const TEST_TOKEN: &str = include_str!("test_token.txt");

    #[test]
    fn test_access_token() {
        let room_config = livekit_protocol::RoomConfiguration {
            name: "name".to_string(),
            agents: vec![livekit_protocol::RoomAgentDispatch {
                agent_name: "test-agent".to_string(),
                metadata: "test-metadata".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let token = AccessToken::with_api_key(TEST_API_KEY, TEST_API_SECRET)
            .with_ttl(Duration::from_secs(60))
            .with_identity("test")
            .with_name("test")
            .with_grants(VideoGrants::default())
            .with_room_config(room_config.clone())
            .to_jwt()
            .unwrap();

        let verifier = TokenVerifier::with_api_key(TEST_API_KEY, TEST_API_SECRET);
        let claims = verifier.verify(&token).unwrap();

        assert_eq!(claims.sub, "test");
        assert_eq!(claims.name, "test");
        assert_eq!(claims.iss, TEST_API_KEY);
        assert_eq!(claims.room_config, Some(room_config));

        let incorrect_issuer = TokenVerifier::with_api_key("incorrect", TEST_API_SECRET);
        assert!(incorrect_issuer.verify(&token).is_err());

        let incorrect_token = TokenVerifier::with_api_key(TEST_API_KEY, "incorrect");
        assert!(incorrect_token.verify(&token).is_err());
    }

    #[test]
    fn test_verify_token_with_room_config() {
        let verifier = TokenVerifier::with_api_key(TEST_API_KEY, TEST_API_SECRET);
        // This token was generated using the Python SDK.
        let claims = verifier.verify(TEST_TOKEN).expect("Failed to verify token.");

        assert_eq!(
            super::Claims {
                sub: "identity".to_string(),
                name: "name".to_string(),
                room_config: Some(livekit_protocol::RoomConfiguration {
                    agents: vec![livekit_protocol::RoomAgentDispatch {
                        agent_name: "test-agent".to_string(),
                        metadata: "test-metadata".to_string(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..claims.clone()
            },
            claims
        );
    }

    #[test]
    fn test_unverified_token() {
        let claims = Claims::from_unverified(TEST_TOKEN).expect("Failed to parse token");

        assert_eq!(claims.sub, "identity");
        assert_eq!(claims.name, "name");
        assert_eq!(claims.iss, TEST_API_KEY);
        assert_eq!(
            claims.room_config,
            Some(livekit_protocol::RoomConfiguration {
                agents: vec![livekit_protocol::RoomAgentDispatch {
                    agent_name: "test-agent".to_string(),
                    metadata: "test-metadata".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            })
        );

        let token = AccessToken::with_api_key(TEST_API_KEY, TEST_API_SECRET)
            .with_ttl(Duration::from_secs(60))
            .with_identity("test")
            .with_name("test")
            .with_grants(VideoGrants {
                room_join: true,
                room: "test-room".to_string(),
                ..Default::default()
            })
            .to_jwt()
            .unwrap();

        let claims = Claims::from_unverified(&token).expect("Failed to parse fresh token");
        assert_eq!(claims.sub, "test");
        assert_eq!(claims.name, "test");
        assert_eq!(claims.video.room, "test-room");
        assert!(claims.video.room_join);

        let parts: Vec<&str> = token.split('.').collect();
        let malformed_token = format!("{}.{}.wrongsignature", parts[0], parts[1]);

        let claims = Claims::from_unverified(&malformed_token)
            .expect("Failed to parse token with wrong signature");
        assert_eq!(claims.sub, "test");
        assert_eq!(claims.name, "test");
    }

    #[test]
    fn test_agent_grant_and_kind() {
        let token = AccessToken::with_api_key(TEST_API_KEY, TEST_API_SECRET)
            .with_ttl(Duration::from_secs(60))
            .with_identity("agent-1")
            .with_kind("agent")
            .with_grants(VideoGrants {
                room_join: true,
                room: "test-room".to_string(),
                agent: true,
                ..Default::default()
            })
            .to_jwt()
            .expect("Failed to create token");

        let verifier = TokenVerifier::with_api_key(TEST_API_KEY, TEST_API_SECRET);
        let claims = verifier.verify(&token).expect("Failed to verify token.");
        assert_eq!(claims.kind, "agent");
        assert!(claims.video.agent);

        let payload = |token: &str| {
            let _ = Claims::from_unverified(token).expect("Failed to parse token");
            jsonwebtoken::dangerous::insecure_decode::<serde_json::Value>(token)
                .expect("Failed to decode token")
                .claims
        };
        let bare = AccessToken::with_api_key(TEST_API_KEY, TEST_API_SECRET)
            .with_ttl(Duration::from_secs(60))
            .with_grants(VideoGrants::default())
            .to_jwt()
            .expect("Failed to create token");
        let p = payload(&bare);
        let mut keys: Vec<&str> = p.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["exp", "iss", "nbf"], "{p}");

        let agent = AccessToken::with_api_key(TEST_API_KEY, TEST_API_SECRET)
            .with_ttl(Duration::from_secs(60))
            .with_grants(VideoGrants { agent: true, can_publish: Some(false), ..Default::default() })
            .to_jwt()
            .expect("Failed to create token");
        let p = payload(&agent);
        assert_eq!(p["video"], serde_json::json!({"agent": true, "canPublish": false}), "{p}");
        let claims = Claims::from_unverified(&agent).expect("Failed to parse token");
        assert!(claims.video.agent && !claims.video.can_publish() && claims.video.can_subscribe());
        assert!(!claims.video.can_publish_data(), "absent canPublishData follows canPublish");
    }

    #[test]
    fn test_defaults_are_not_serialized() {
        assert_eq!(serde_json::to_string(&VideoGrants::default()).unwrap(), "{}");
        let parsed: VideoGrants = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, VideoGrants::default());
        assert_eq!(serde_json::to_string(&parsed).unwrap(), "{}");
        let explicit = VideoGrants { can_publish: Some(false), can_publish_data: Some(true), ..Default::default() };
        assert_eq!(
            serde_json::to_string(&explicit).unwrap(),
            r#"{"canPublish":false,"canPublishData":true}"#
        );
        assert!(explicit.can_publish_data());

        assert_eq!(serde_json::to_string(&SIPGrants::default()).unwrap(), "{}");
        let parsed: SIPGrants = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, SIPGrants::default());
        assert_eq!(serde_json::to_string(&parsed).unwrap(), "{}");

        let claims = Claims { exp: 1, iss: "k".to_string(), nbf: 0, ..Default::default() };
        let json = r#"{"exp":1,"iss":"k","nbf":0}"#;
        assert_eq!(serde_json::to_string(&claims).unwrap(), json);
        let parsed: Claims = serde_json::from_str(json).unwrap();
        assert_eq!(parsed, claims);
        assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
    }
}
