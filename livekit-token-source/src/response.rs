use crate::error::TokenSourceError;

#[derive(serde::Deserialize)]
pub struct TokenSourceResponse {
    pub server_url: String,
    pub participant_token: String,
}

impl TokenSourceResponse {
    pub fn new(server_url: String, participant_token: String) -> TokenSourceResponse {
        TokenSourceResponse{server_url: server_url, participant_token: participant_token}
    }
}

pub type TokenSourceResult<T> = Result<T, TokenSourceError>;