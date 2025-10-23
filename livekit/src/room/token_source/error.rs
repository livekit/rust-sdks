use std::error::Error;

use livekit_api::access_token::AccessTokenError;

#[derive(Debug, thiserror::Error)]
pub enum TokenSourceError {
    #[error("network error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error(
        "Error generating token from endpoint {url}: received {status_code:?} / {raw_body_text}"
    )]
    TokenGenerationFailed { url: String, status_code: reqwest::StatusCode, raw_body_text: String },
    #[error("access token error: {0}")]
    AccessToken(#[from] AccessTokenError),
    #[error("Other error: {0}")]
    Other(#[from] Box<dyn Error + Send + Sync>),
}

pub type TokenSourceResult<T> = Result<T, TokenSourceError>;
