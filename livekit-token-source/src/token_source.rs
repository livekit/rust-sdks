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

use std::collections::HashMap;

use crate::caching::TokenSourceCached;
use crate::error::TokenSourceError;
use crate::request::{TokenSourceFetchOptions, TokenSourceRequest};
use crate::response::{TokenSourceResponse, TokenSourceResult};
use async_trait::async_trait;
use livekit_net::{Header, HttpClientExt};

const DEVELOPMENT_TOKEN_SERVER_ENDPOINT_URL: &str =
    "https://cloud-api.livekit.io/api/v2/sandbox/connection-details";
const DEVELOPMENT_TOKEN_SERVER_ID_HEADER: &str = "X-Sandbox-ID";

/// A token source whose credentials are not parameterized: `fetch` takes no
/// options and every call resolves the same way.
#[async_trait]
pub trait TokenSourceFixed {
    async fn fetch(&self) -> TokenSourceResult<TokenSourceResponse>;
}

/// The return type of [`literal`].
pub struct TokenSourceLiteral {
    response: TokenSourceResponse,
}

#[async_trait]
impl TokenSourceFixed for TokenSourceLiteral {
    async fn fetch(&self) -> TokenSourceResult<TokenSourceResponse> {
        Ok(self.response.clone())
    }
}

/// Creates a token source holding a single, literal set of credentials,
/// returned as-is on every fetch.
pub fn literal(
    server_url: impl Into<String>,
    participant_token: impl Into<String>,
) -> TokenSourceLiteral {
    TokenSourceLiteral {
        response: TokenSourceResponse {
            server_url: server_url.into(),
            participant_token: participant_token.into(),
        },
    }
}

/// A token source that generates credentials from per-call
/// [`TokenSourceFetchOptions`] (room name, participant identity, agent
/// dispatch, ...).
///
/// Implement this trait to plug a custom credential backend into code that is
/// generic over token sources.
#[async_trait]
pub trait TokenSourceConfigurable {
    async fn fetch(
        &self,
        options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse>;

    /// Wraps this source in a caching layer that stores the last fetched
    /// credentials and serves them for repeat fetches with equal options, for
    /// as long as the token stays valid. See [`TokenSourceCached`].
    fn cached(self) -> TokenSourceCached<Self>
    where
        Self: Sized + Send + Sync,
    {
        TokenSourceCached::new(self)
    }
}

/// The return type of [`endpoint`].
pub struct TokenSourceEndpoint {
    endpoint_url: String,
    headers: HashMap<String, String>,
}

#[async_trait]
impl TokenSourceConfigurable for TokenSourceEndpoint {
    async fn fetch(
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

/// Creates a token source that fetches credentials from the given URL
/// using the standard token endpoint format.
///
/// The given headers are sent along with every request, e.g. for
/// authentication against the endpoint.
///
/// See <https://docs.livekit.io/frontends/build/authentication/endpoint/>
/// for the endpoint contract.
pub fn endpoint(endpoint_url: impl Into<String>) -> TokenSourceEndpoint {
    TokenSourceEndpoint { endpoint_url: endpoint_url.into(), headers: HashMap::new() }
}

impl TokenSourceEndpoint {
    /// Adds a single header to the headers sent with every request, keeping any set previously.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Adds the given headers to the headers sent with every request, keeping any set previously.
    /// A key that was already set is overwritten with its new value.
    pub fn with_headers(
        mut self,
        value: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        self.headers.extend(value.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }
}

/// The return type of [`development_token_server`].
pub struct TokenSourceDevelopmentTokenServer {
    token_source_endpoint: TokenSourceEndpoint,
}

#[async_trait]
impl TokenSourceConfigurable for TokenSourceDevelopmentTokenServer {
    async fn fetch(
        &self,
        options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse> {
        self.token_source_endpoint.fetch(options).await
    }
}

/// Creates a token source that queries a LiveKit development token server
/// for credentials, for quick prototyping / getting-started use cases.
///
/// **This token provider is INSECURE and should NOT be used in
/// production.**
///
/// See <https://docs.livekit.io/frontends/build/authentication/sandbox-token-server/>.
pub fn development_token_server(
    token_server_id: impl Into<String>,
) -> TokenSourceDevelopmentTokenServer {
    TokenSourceDevelopmentTokenServer {
        token_source_endpoint: endpoint(DEVELOPMENT_TOKEN_SERVER_ENDPOINT_URL)
            .with_header(DEVELOPMENT_TOKEN_SERVER_ID_HEADER, token_server_id),
    }
}
