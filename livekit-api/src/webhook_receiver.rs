use crate::{access_token::TokenVerifier, proto};

#[derive(Clone, Debug)]
pub struct WebhookReceiver {
    token_verifier: TokenVerifier,
}

impl WebhookReceiver {
    pub fn new(token_verifier: TokenVerifier) -> Self {
        Self { token_verifier }
    }

    pub fn receive(&self) -> proto::WebhookEvent {
        

    }
}
