pub struct TokenSourceLiteral {
    // pub result: TokenSourceResponse
    pub result: TokenSourceResult<TokenSourceResponse>
    // pub a: i32,
}

impl TokenSourceLiteral {
    pub fn new() -> TokenSourceLiteral {
        // TokenSourceLiteral {result: TokenSourceResponse::new()}
        TokenSourceLiteral {
            // result: Ok(TokenSourceResponse::new())
            result: Err(TokenSourceError::ErrorA)
        }
    }
}

pub struct TokenSourceResponse {
    pub server_url: String,
    pub participant_token: String,
}

impl TokenSourceResponse {
    pub fn new() -> TokenSourceResponse {
        TokenSourceResponse{server_url: "abc".to_string(), participant_token: "def".to_string()}
    }
}

pub type TokenSourceResult<T> = Result<T, TokenSourceError>;

  #[derive(Debug, thiserror::Error)]
  pub enum TokenSourceError {
      #[error("error A occurred")]
      ErrorA,
      #[error("error B occurred")]
      ErrorB,
  }