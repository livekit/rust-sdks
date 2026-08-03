use crate::request::TokenSourceRequest;
use crate::request::TokenSourceFetchOptions;
use crate::response::TokenSourceResponse;
use crate::response::TokenSourceResult;
use crate::error::TokenSourceError;
use livekit_net::{Header, HttpClientExt};

const DEVELOPMENT_TOKEN_SERVER_ENDPOINT_URL: &str = "https://cloud-api.livekit.io/api/v2/sandbox/connection-details";
const DEVELOPMENT_TOKEN_SERVER_ID_HEADER: &str = "X-Sandbox-ID";

pub struct TokenSourceLiteral {
    result: TokenSourceResult<TokenSourceResponse>
}

impl TokenSourceLiteral {
    pub fn new(response: TokenSourceResponse) -> TokenSourceLiteral {
        TokenSourceLiteral { result: Ok(response) }
    }
    pub fn fetch(&self) -> &TokenSourceResult<TokenSourceResponse> { &self.result }
}

pub struct TokenSourceEndpoint {
    endpoint_url: String,
    headers: Vec<(String, String)>,
}

impl TokenSourceEndpoint {
    pub fn new(endpoint_url: impl Into<String>, headers: Vec<(String, String)>) -> TokenSourceEndpoint {
        TokenSourceEndpoint{
            endpoint_url: endpoint_url.into(),
            headers,
        }
    }

    pub async fn fetch(&self, options: &TokenSourceFetchOptions) -> TokenSourceResult<TokenSourceResponse> {
        let request = TokenSourceRequest::from(options);

        let http_client = livekit_net::http_client().ok_or(TokenSourceError::TransportNotConfigured)?;

        let body = serde_json::to_vec(&request)?;
        let mut headers = vec![Header { name: "Content-Type".into(), value: "application/json".into() }];
        headers.extend(self.headers.iter().map(|(name, value)| Header { name: name.clone(), value: value.clone() }));

        let response = http_client.post(self.endpoint_url.clone(), headers, body).await?;

        if !(200..300).contains(&response.status) {
            return Err(TokenSourceError::Server {
                status: response.status,
                body: String::from_utf8_lossy(&response.body).into_owned()
            });
        }

        let connection_details = serde_json::from_slice::<TokenSourceResponse>(&response.body)?;
        Ok(connection_details)
    }
}

pub struct TokenSourceDevelopmentTokenServer {
    token_source_endpoint: TokenSourceEndpoint
}

impl TokenSourceDevelopmentTokenServer {
    pub fn new(token_server_id: String) -> TokenSourceDevelopmentTokenServer { 
        let token_source_endpoint = TokenSourceEndpoint::new(
            DEVELOPMENT_TOKEN_SERVER_ENDPOINT_URL,
            vec![(DEVELOPMENT_TOKEN_SERVER_ID_HEADER.to_string(), token_server_id)]
        );
        
        TokenSourceDevelopmentTokenServer { 
            token_source_endpoint
        }
    }
    pub async fn fetch(&self, options: &TokenSourceFetchOptions) ->  TokenSourceResult<TokenSourceResponse> {
        self.token_source_endpoint.fetch(options).await
    }
}