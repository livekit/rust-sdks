pub struct TokenSourceLiteral {
    result: TokenSourceResult<TokenSourceResponse>
}

impl TokenSourceLiteral {
    pub fn new(response: TokenSourceResponse) -> TokenSourceLiteral {
        TokenSourceLiteral { result: Ok(response) }
    }
    pub fn fetch(&self) -> &TokenSourceResult<TokenSourceResponse> { &self.result }
}

pub struct TokenSourceSandbox {
    sandbox_id: String
}

impl TokenSourceSandbox {
    pub fn new(sandbox_id: String) -> TokenSourceSandbox { 
        TokenSourceSandbox { sandbox_id }  
    }
    pub async fn fetch(&self) -> &TokenSourceResult<TokenSourceResponse> {
        
    }
}

// ================================================================================

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

  #[derive(Debug, thiserror::Error)]
  pub enum TokenSourceError {
      #[error("error A occurred")]
      ErrorA,
      #[error("error B occurred")]
      ErrorB,
  }