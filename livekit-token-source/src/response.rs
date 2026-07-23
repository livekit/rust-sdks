use crate::error::TokenSourceError;

#[derive(serde::Deserialize)]
pub struct TokenSourceResponse {
    pub server_url: String,
    pub participant_token: String,
}

pub type TokenSourceResult<T> = Result<T, TokenSourceError>;