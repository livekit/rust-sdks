use crate::request::TokenSourceRequest;
use crate::request::TokenSourceFetchOptions;
use crate::response::TokenSourceResponse;
use crate::response::TokenSourceResult;
use crate::error::TokenSourceError;

pub struct TokenSourceLiteral {
    result: TokenSourceResult<TokenSourceResponse>
}

impl TokenSourceLiteral {
    pub fn new(response: TokenSourceResponse) -> TokenSourceLiteral {
        return TokenSourceLiteral { result: Ok(response) };
    }
    pub fn fetch(&self) -> &TokenSourceResult<TokenSourceResponse> { return &self.result; }
}

pub struct TokenSourceEndpoint {
    endpoint_url: String,
    header: (String, String),
    http_client: reqwest::Client,
}

impl TokenSourceEndpoint {
    pub fn new(endpoint_url: String, header: (String, String)) -> TokenSourceEndpoint {
        let http_client = reqwest::Client::new();
        
        return TokenSourceEndpoint{
            endpoint_url, 
            header,
            http_client
        };
    }

    pub async fn fetch(&self, options: &TokenSourceFetchOptions) -> TokenSourceResult<TokenSourceResponse> {
        let request = TokenSourceRequest::from(options);
        let response = self.http_client
            .post(self.endpoint_url.as_str())
            .header(self.header.0.as_str(), self.header.1.as_str())
            .json(&request)
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(TokenSourceError::Server { 
                status: response.status().as_u16(), 
                body: response.text().await.unwrap_or_default() 
            });
        }  

        let connection_details = response.json::<TokenSourceResponse>().await?;

        return Ok(
            connection_details
        )
    }
}

pub struct TokenSourceSandbox {
    token_source_endpoint: TokenSourceEndpoint
}

impl TokenSourceSandbox {
    pub fn new(sandbox_id: String) -> TokenSourceSandbox { 
        let token_source_endpoint = TokenSourceEndpoint::new(
            "https://cloud-api.livekit.io/api/v2/sandbox/connection-details".to_string(),
            ("X-Sandbox-ID".to_string(), sandbox_id)
        );
        
        return TokenSourceSandbox { 
            token_source_endpoint
        };
    }
    pub async fn fetch(&self, options: &TokenSourceFetchOptions) ->  TokenSourceResult<TokenSourceResponse> {
        return self.token_source_endpoint.fetch(options).await;
    }
}