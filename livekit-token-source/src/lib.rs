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
        return TokenSourceLiteral { result: Ok(response) };
        // return TokenSourceLiteral { result: Err(error::TokenSourceError::ErrorA) }
    }
    pub fn fetch(self) -> TokenSourceResult<TokenSourceResponse> { return self.result; }
}

pub struct TokenSourceEndpoint {
    endpoint_url: String,
    header: (String, String)
}

impl TokenSourceEndpoint {
    pub fn new(endpoint_url: String, header: (String, String)) -> TokenSourceEndpoint {
        return TokenSourceEndpoint{endpoint_url, header};
    }

    pub async fn fetch(self) -> TokenSourceResult<TokenSourceResponse> {
        let http_client = reqwest::Client::new();
        let response = http_client
            .post(self.endpoint_url)
            .header(self.header.0, self.header.1)
            .send()
            .await?;
        
        let connection_details = response.json::<TokenSourceResponse>().await?;

        return Ok(
            connection_details
        )
    }
}

pub struct TokenSourceSandbox {
    sandbox_id: String
}

impl TokenSourceSandbox {
    pub fn new(sandbox_id: String) -> TokenSourceSandbox { 
        return TokenSourceSandbox { sandbox_id };
    }
    pub async fn fetch(self) ->  TokenSourceResult<TokenSourceResponse> {
        let token_source_endpoint = TokenSourceEndpoint::new(
            "https://cloud-api.livekit.io/api/v2/sandbox/connection-details".to_string(),
            ("X-Sandbox-ID".to_string(), self.sandbox_id)
        );
        
        return token_source_endpoint.fetch().await;
    }
}