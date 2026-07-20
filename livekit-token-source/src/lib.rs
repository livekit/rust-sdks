mod error;
mod request;
mod response;

pub use response::TokenSourceResponse;
pub use response::TokenSourceResult;

pub struct TokenSourceLiteral {
    result: TokenSourceResult<TokenSourceResponse>
}

impl TokenSourceLiteral {
    pub fn new(response: TokenSourceResponse) -> TokenSourceLiteral {
        TokenSourceLiteral { result: Ok(response) }
    }
    pub fn fetch(self) -> TokenSourceResult<TokenSourceResponse> { self.result }
}

pub struct TokenSourceSandbox {
    sandbox_id: String
}

impl TokenSourceSandbox {
    pub fn new(sandbox_id: String) -> TokenSourceSandbox { 
        TokenSourceSandbox { sandbox_id }  
    }
    // pub async fn fetch ... 
}