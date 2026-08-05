// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::error::TokenSourceError;
use crate::request::TokenSourceFetchOptions;
use crate::request::TokenSourceRequest;
use crate::response::TokenSourceResponse;
use crate::response::TokenSourceResult;
use livekit_net::{Header, HttpClientExt};

const DEVELOPMENT_TOKEN_SERVER_ENDPOINT_URL: &str =
    "https://cloud-api.livekit.io/api/v2/sandbox/connection-details";
const DEVELOPMENT_TOKEN_SERVER_ID_HEADER: &str = "X-Sandbox-ID";

pub enum TokenSource {}

impl TokenSource {
    pub fn literal(response: TokenSourceResponse) -> TokenSourceLiteral {
        TokenSourceLiteral { result: Ok(response) }
    }

    pub fn endpoint(
        endpoint_url: impl Into<String>,
        headers: Vec<(String, String)>,
    ) -> TokenSourceEndpoint {
        TokenSourceEndpoint { endpoint_url: endpoint_url.into(), headers }
    }

    pub fn development_token_server(token_server_id: String) -> TokenSourceDevelopmentTokenServer {
        TokenSourceDevelopmentTokenServer {
            token_source_endpoint: TokenSource::endpoint(
                DEVELOPMENT_TOKEN_SERVER_ENDPOINT_URL,
                vec![(DEVELOPMENT_TOKEN_SERVER_ID_HEADER.to_string(), token_server_id)],
            ),
        }
    }
}

pub struct TokenSourceLiteral {
    result: TokenSourceResult<TokenSourceResponse>,
}

impl TokenSourceLiteral {
    pub fn fetch(&self) -> &TokenSourceResult<TokenSourceResponse> {
        &self.result
    }
}

pub struct TokenSourceEndpoint {
    endpoint_url: String,
    headers: Vec<(String, String)>,
}

impl TokenSourceEndpoint {
    pub async fn fetch(
        &self,
        options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse> {
        let request = TokenSourceRequest::from(options);

        let http_client =
            livekit_net::http_client().ok_or(TokenSourceError::TransportNotConfigured)?;

        let body = serde_json::to_vec(&request)?;
        let mut headers =
            vec![Header { name: "Content-Type".into(), value: "application/json".into() }];
        headers.extend(
            self.headers
                .iter()
                .map(|(name, value)| Header { name: name.clone(), value: value.clone() }),
        );

        let response = http_client.post(self.endpoint_url.clone(), headers, body).await?;

        if !(200..300).contains(&response.status) {
            return Err(TokenSourceError::Server {
                status: response.status,
                body: String::from_utf8_lossy(&response.body).into_owned(),
            });
        }

        let connection_details = serde_json::from_slice::<TokenSourceResponse>(&response.body)?;
        Ok(connection_details)
    }
}

pub struct TokenSourceDevelopmentTokenServer {
    token_source_endpoint: TokenSourceEndpoint,
}

impl TokenSourceDevelopmentTokenServer {
    pub async fn fetch(
        &self,
        options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse> {
        self.token_source_endpoint.fetch(options).await
    }
}
