use crate::access_token::{AccessTokenError, TokenVerifier};
use livekit_protocol as proto;
use serde_json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WebhookError {
    #[error("invalid signature")]
    InvalidSignature,
    #[error("failed to verify the authorization: {0}")]
    InvalidAuth(#[from] AccessTokenError),
}

#[derive(Clone, Debug)]
pub struct WebhookReceiver {
    token_verifier: TokenVerifier,
}

impl WebhookReceiver {
    pub fn new(token_verifier: TokenVerifier) -> Self {
        Self { token_verifier }
    }

    // Auth is found on the Authorization header
    pub fn receive(&self, body: &str, auth: &str) -> Result<proto::WebhookEvent, WebhookError> {
        // Validate auth
        let claims = self.token_verifier.verify(auth)?;

        // Validate sha256 signature from claims

        // Prost doesn't support serde_json deserialization
        // So deserialize manually

        let event = proto::WebhookEvent {};
        event
    }
}
