// Copyright 2025 LiveKit, Inc.
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

use livekit_api::access_token::{self, AccessTokenError};
use livekit_protocol as proto;
use std::{future::Future, pin::Pin};

mod cache;
mod error;
mod fetch_options;
mod minter_credentials;
mod request_response;
mod token_response_cache;
mod traits;

pub use cache::{CacheConfigurable, CacheFixed, TokenSourceCache};
pub use error::{TokenSourceError, TokenSourceResult};
pub use fetch_options::TokenSourceFetchOptions;
pub use minter_credentials::{
    MinterCredentials, MinterCredentialsEnvironment, MinterCredentialsSource,
};
pub use request_response::{TokenSourceRequest, TokenSourceResponse};
pub use token_response_cache::{
    TokenResponseCache, TokenResponseCacheValue, TokenResponseInMemoryCache,
};
pub use traits::{
    TokenSourceConfigurable, TokenSourceConfigurableSynchronous, TokenSourceFixed,
    TokenSourceFixedSynchronous,
};

pub trait TokenLiteralGenerator {
    fn apply(&self) -> TokenSourceResponse;
}

impl TokenLiteralGenerator for TokenSourceResponse {
    fn apply(&self) -> TokenSourceResponse {
        self.clone()
    }
}

impl<F: Fn() -> TokenSourceResponse> TokenLiteralGenerator for F {
    // FIXME: allow this to be an async fn!
    fn apply(&self) -> TokenSourceResponse {
        self()
    }
}

pub struct TokenSourceLiteral<Generator: TokenLiteralGenerator> {
    generator: Generator,
}
impl<G: TokenLiteralGenerator> TokenSourceLiteral<G> {
    pub fn new(generator: G) -> Self {
        Self { generator }
    }
}

impl<G: TokenLiteralGenerator> TokenSourceFixedSynchronous for TokenSourceLiteral<G> {
    fn fetch_synchronous(&self) -> TokenSourceResult<TokenSourceResponse> {
        Ok(self.generator.apply())
    }
}

pub struct TokenSourceMinter<CredentialsSource: MinterCredentialsSource> {
    credentials_source: CredentialsSource,
}

impl<CS: MinterCredentialsSource> TokenSourceMinter<CS> {
    pub fn new(credentials_source: CS) -> Self {
        Self { credentials_source }
    }
}

impl<C: MinterCredentialsSource> TokenSourceConfigurableSynchronous for TokenSourceMinter<C> {
    fn fetch_synchronous(
        &self,
        options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse> {
        let MinterCredentials { url: server_url, api_key, api_secret } =
            self.credentials_source.get();

        // FIXME: apply options in the below code!
        let participant_token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
            .with_identity("rust-bot")
            .with_name("Rust Bot")
            .with_grants(access_token::VideoGrants {
                room_join: true,
                room: "my-room".to_string(),
                ..Default::default()
            })
            .to_jwt()?;

        Ok(TokenSourceResponse { server_url, participant_token })
    }
}

impl Default for TokenSourceMinter<MinterCredentialsEnvironment> {
    fn default() -> Self {
        Self::new(MinterCredentialsEnvironment::default())
    }
}

pub struct TokenSourceCustomMinter<
    MintFn: Fn(access_token::AccessToken) -> Result<String, AccessTokenError>,
    Credentials: MinterCredentialsSource,
> {
    mint_fn: MintFn,
    credentials_source: Credentials,
}

impl<
        MF: Fn(access_token::AccessToken) -> Result<String, AccessTokenError>,
        CS: MinterCredentialsSource,
    > TokenSourceCustomMinter<MF, CS>
{
    pub fn new_with_credentials(mint_fn: MF, credentials_source: CS) -> Self {
        Self { mint_fn, credentials_source }
    }
    pub fn new(mint_fn: MF) -> TokenSourceCustomMinter<MF, MinterCredentialsEnvironment> {
        TokenSourceCustomMinter::new_with_credentials(
            mint_fn,
            MinterCredentialsEnvironment::default(),
        )
    }
}

impl<
        MF: Fn(access_token::AccessToken) -> Result<String, AccessTokenError>,
        C: MinterCredentialsSource,
    > TokenSourceFixedSynchronous for TokenSourceCustomMinter<MF, C>
{
    fn fetch_synchronous(&self) -> TokenSourceResult<TokenSourceResponse> {
        let MinterCredentials { url: server_url, api_key, api_secret } =
            self.credentials_source.get();

        let participant_token =
            (self.mint_fn)(access_token::AccessToken::with_api_key(&api_key, &api_secret))?;

        Ok(TokenSourceResponse { server_url, participant_token })
    }
}

use reqwest::{header::HeaderMap, Method};

pub struct TokenSourceEndpoint {
    url: String,
    method: Method,
    headers: HeaderMap,
}

impl TokenSourceEndpoint {
    pub fn new(url: &str) -> Self {
        Self { url: url.into(), method: Method::POST, headers: HeaderMap::new() }
    }
}

impl TokenSourceConfigurable for TokenSourceEndpoint {
    async fn fetch(
        &self,
        options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse> {
        let client = reqwest::Client::new();

        let request: TokenSourceRequest = options.clone().into();
        let request_proto: proto::TokenSourceRequest = request.into();

        let response = client
            .request(self.method.clone(), &self.url)
            .json(&request_proto)
            .headers(self.headers.clone())
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(TokenSourceError::TokenGenerationFailed {
                url: self.url.clone(),
                status_code: response.status(),
                raw_body_text: response.text().await?,
            });
        }

        let response_proto = response.json::<proto::TokenSourceResponse>().await?;
        Ok(response_proto.into())
    }
}

pub struct TokenSourceSandboxTokenServer(TokenSourceEndpoint);

impl TokenSourceSandboxTokenServer {
    pub fn new(sandbox_id: &str) -> Self {
        Self::new_with_base_url(sandbox_id, "https://cloud-api.livekit.io")
    }
    pub fn new_with_base_url(sandbox_id: &str, base_url: &str) -> Self {
        let mut endpoint =
            TokenSourceEndpoint::new(&format!("{base_url}/api/v2/sandbox/connection-details"));
        endpoint.headers.insert("X-Sandbox-ID", sandbox_id.parse().unwrap());
        Self(endpoint)
    }
}

impl TokenSourceConfigurable for TokenSourceSandboxTokenServer {
    async fn fetch(
        &self,
        options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse> {
        self.0.fetch(options).await
    }
}

pub struct TokenSourceCustom<
    CustomFn: Fn(
        &TokenSourceFetchOptions,
    ) -> Pin<Box<dyn Future<Output = TokenSourceResult<TokenSourceResponse>>>>,
>(CustomFn);

impl<
        CustomFn: Fn(
            &TokenSourceFetchOptions,
        ) -> Pin<Box<dyn Future<Output = TokenSourceResult<TokenSourceResponse>>>>,
    > TokenSourceCustom<CustomFn>
{
    pub fn new(custom_fn: CustomFn) -> Self {
        Self(custom_fn)
    }
}

impl<
        CustomFn: Fn(
            &TokenSourceFetchOptions,
        ) -> Pin<Box<dyn Future<Output = TokenSourceResult<TokenSourceResponse>>>>,
    > TokenSourceConfigurable for TokenSourceCustom<CustomFn>
{
    async fn fetch(
        &self,
        options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse> {
        (self.0)(options).await
    }
}
