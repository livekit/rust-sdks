use crate::access_token::{AccessTokenError, TokenVerifier};
use base64::Engine;
use livekit_protocol as proto;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WebhookError {
    #[error("invalid signature")]
    InvalidSignature,
    #[error("invalid base64")]
    InvalidBase64(#[from] base64::DecodeError),
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

        let mut hasher = Sha256::new();
        hasher.update(body);
        let hash = hasher.finalize();

        let claim_hash = base64::engine::general_purpose::STANDARD.decode(&claims.sha256)?;
        if &claim_hash[..] != &hash[..] {
            return Err(WebhookError::InvalidSignature);
        }

        Ok(serde_json::from_str(body)?)
    }
}
