use jsonwebtoken::{self, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::env;
use std::fmt::Debug;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

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
    Encode(#[from] jsonwebtoken::errors::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoGrants {
    // actions on rooms
    room_create: bool,
    room_list: bool,
    room_record: bool,

    // actions on a particular room
    room_admin: bool,
    room_join: bool,
    room: String,

    // permissions within a room
    can_publish: bool,
    can_subscribe: bool,
    can_publish_data: bool,

    // TrackSource types that a participant may publish.
    // When set, it supercedes CanPublish. Only sources explicitly set here can be published
    can_publish_sources: Vec<String>, // keys keep track of each source

    // by default, a participant is not allowed to update its own metadata
    can_update_own_metadata: bool,

    // actions on ingresses
    ingress_admin: bool, // applies to all ingress

    // participant is not visible to other participants (useful when making bots)
    hidden: bool,

    // indicates to the room that current participant is a recorder
    recorder: bool,
}

impl Default for VideoGrants {
    fn default() -> Self {
        Self {
            room_create: false,
            room_list: false,
            room_record: false,
            room_admin: false,
            room_join: true,
            room: "".to_string(),
            can_publish: true,
            can_subscribe: true,
            can_publish_data: true,
            can_publish_sources: Vec::default(),
            can_update_own_metadata: false,
            ingress_admin: false,
            hidden: false,
            recorder: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    exp: usize,  // Expiration
    iss: String, // ApiKey
    nbf: usize,
    sub: String, // Identity

    name: String,
    video: VideoGrants,
    sha256: String, // Used to verify the integrity of the message body
    metadata: String,
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
    pub fn with_key(api_key: &str, api_secret: &str) -> Self {
        Self {
            api_key: api_key.to_owned(),
            api_secret: api_secret.to_owned(),
            claims: Claims {
                exp: DEFAULT_TTL.as_secs() as usize,
                iss: api_key.to_owned(),
                nbf: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as usize,
                sub: Default::default(),
                name: Default::default(),
                video: VideoGrants::default(),
                sha256: Default::default(),
                metadata: Default::default(),
            },
        }
    }

    pub fn new() -> Result<Self, AccessTokenError> {
        // Try to get the API Key and the Secret Key from the environment
        let api_key = env::var("LIVEKIT_API_KEY")?;
        let api_secret = env::var("LIVEKIT_API_SECRET")?;
        Ok(Self::with_key(&api_key, &api_secret))
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap() + ttl;
        self.claims.exp = time.as_secs() as usize;
        self
    }

    pub fn with_identity(mut self, identity: String) -> Self {
        self.claims.sub = identity;
        self
    }

    pub fn with_name(mut self, name: String) -> Self {
        self.claims.name = name;
        self
    }

    pub fn with_metadata(mut self, metadata: String) -> Self {
        self.claims.metadata = metadata;
        self
    }

    pub fn to_jwt(self) -> Result<String, AccessTokenError> {
        if self.api_key.is_empty() || self.api_secret.is_empty() {
            return Err(AccessTokenError::InvalidKeys);
        }

        if self.claims.sub.is_empty() && self.claims.video.room_join.is_empty() {
            return Err(AccessTokenError::InvalidClaims(
                "token grants room_join but doesn't have an identity",
            )));
        }

        Ok(jsonwebtoken::encode(
            &Header::new(jsonwebtoken::Algorithm::HS256),
            &self.claims,
            &EncodingKey::from_secret(self.api_secret.as_ref()),
        )?)
    }
}
