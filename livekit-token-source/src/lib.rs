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
        // return TokenSourceLiteral { result: Ok(response) }
        return TokenSourceLiteral { result: Err(error::TokenSourceError::ErrorA) }
    }
    pub fn fetch(self) -> TokenSourceResult<TokenSourceResponse> { return self.result }
}

pub struct TokenSourceSandbox {
    sandbox_id: String
}

impl TokenSourceSandbox {
    pub fn new(sandbox_id: String) -> TokenSourceSandbox { 
        return TokenSourceSandbox { sandbox_id }  
    }
    pub async fn fetch(self) ->  TokenSourceResult<TokenSourceResponse> {
        let http_client = reqwest::Client::new();
        let response = http_client
            .post("https://cloud-api.livekit.io/api/v2/sandbox/connection-details")
            .header("X-Sandbox-ID", self.sandbox_id)
            .send()
            .await?;
        
        let connection_details = response.json::<TokenSourceResponse>().await?;

        return Ok(
            connection_details
        )
    }
}