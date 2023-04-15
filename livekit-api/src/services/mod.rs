use crate::access_token::{AccessToken, AccessTokenError, VideoGrants};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use std::fmt::Debug;
use thiserror::Error;

pub mod room_client;
pub use room_client::*;

mod twirp_client;

pub const LIVEKIT_PACKAGE: &'static str = "livekit";

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("invalid environment: {0}")]
    Env(#[from] std::env::VarError),
    #[error("invalid access token: {0}")]
    AccessToken(#[from] AccessTokenError),
    #[error("twirp error: {0}")]
    Twirp(#[from] twirp_client::TwirpError),
}

pub type ServiceResult<T> = Result<T, ServiceError>;

struct ServiceBase {
    api_key: String,
    api_secret: String,
}

impl Debug for ServiceBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceBase")
            .field("api_key", &self.api_key)
            .finish()
    }
}

impl ServiceBase {
    pub fn with_api_key(api_key: &str, api_secret: &str) -> Self {
        Self {
            api_key: api_key.to_owned(),
            api_secret: api_secret.to_owned(),
        }
    }

    pub fn auth_header(&self, grants: VideoGrants) -> Result<HeaderMap, AccessTokenError> {
        let token = AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_grants(grants)
            .to_jwt()?;

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        );
        Ok(headers)
    }
}
