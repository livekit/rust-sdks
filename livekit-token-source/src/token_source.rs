use crate::request::TokenSourceRequest;
use crate::request::TokenSourceFetchOptions;
use crate::response::TokenSourceResponse;
use crate::error::TokenSourceError;

#[derive(uniffi::Object)]
pub struct TokenSourceLiteral {
    response: TokenSourceResponse
}

#[uniffi::export]
impl TokenSourceLiteral {
    #[uniffi::constructor]
    pub fn new(response: TokenSourceResponse) -> TokenSourceLiteral {
        return TokenSourceLiteral { response };
    }
    pub fn fetch(&self) -> Result<TokenSourceResponse, TokenSourceError> {
        return Ok(self.response.clone());
    }
}

#[derive(uniffi::Object)]
pub struct TokenSourceEndpoint {
    endpoint_url: String,
    header: (String, String),
    http_client: reqwest::Client,
}

#[uniffi::export(async_runtime = "tokio")]
impl TokenSourceEndpoint {
    #[uniffi::constructor]
    pub fn new(endpoint_url: String, header_name: String, header_value: String) -> TokenSourceEndpoint {
        let http_client = reqwest::Client::new();

        return TokenSourceEndpoint{
            endpoint_url,
            header: (header_name, header_value),
            http_client
        };
    }

    pub async fn fetch(&self, options: TokenSourceFetchOptions) -> Result<TokenSourceResponse, TokenSourceError> {
        let request = TokenSourceRequest::from(&options);
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

#[derive(uniffi::Object)]
pub struct TokenSourceSandbox {
    token_source_endpoint: TokenSourceEndpoint
}

#[uniffi::export(async_runtime = "tokio")]
impl TokenSourceSandbox {
    #[uniffi::constructor]
    pub fn new(sandbox_id: String) -> TokenSourceSandbox {
        let token_source_endpoint = TokenSourceEndpoint::new(
            "https://cloud-api.livekit.io/api/v2/sandbox/connection-details".to_string(),
            "X-Sandbox-ID".to_string(),
            sandbox_id
        );

        return TokenSourceSandbox {
            token_source_endpoint
        };
    }
    pub async fn fetch(&self, options: TokenSourceFetchOptions) ->  Result<TokenSourceResponse, TokenSourceError> {
        return self.token_source_endpoint.fetch(options).await;
    }
}
