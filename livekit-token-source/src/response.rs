use crate::error::TokenSourceError;

#[derive(Clone, serde::Deserialize, uniffi::Record)]
pub struct TokenSourceResponse {
    pub server_url: String,
    pub participant_token: String,
}

pub type TokenSourceResult<T> = Result<T, TokenSourceError>;