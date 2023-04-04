use crate::access_token::{AccessTokenError, TokenVerifier};
use livekit_protocol as proto;
use serde_json;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WebhookError {
    #[error("invalid signature")]
    InvalidSignature,
    #[error("failed to verify the authorization: {0}")]
    InvalidAuth(#[from] AccessTokenError),
    #[error("invalid body, failed to decode: {0}")]
    InvalidData(#[from] serde_json::Error),
}

#[derive(Clone, Debug)]
pub struct WebhookReceiver {
    token_verifier: TokenVerifier,
}

impl WebhookReceiver {
    pub fn new(token_verifier: TokenVerifier) -> Self {
        Self { token_verifier }
    }

    pub fn receive(
        &self,
        body: &str,
        auth_token: &str,
    ) -> Result<proto::WebhookEvent, WebhookError> {
        let claims = self.token_verifier.verify(auth_token)?;

        let hasher = Sha256::new();
        hasher.update(body);
        let hash = hasher.finalize();

        let hex: Result<Vec<u8>, std::num::ParseIntError> = (0..claims.sha256.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&claims.sha256[i..i + 2], 16))
            .collect();

        let hex = hex.map_err(|_| WebhookError::InvalidSignature)?; // Failed to parse

        if &hex[..] != &hash[..] {
            return Err(WebhookError::InvalidSignature);
        }

        Ok(serde_json::from_str(body)?)
    }
}
