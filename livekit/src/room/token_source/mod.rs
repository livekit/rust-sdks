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

use std::{error::Error, future::{ready, Future}, pin::Pin};
use livekit_api::access_token::{self, AccessTokenError};

// FIXME: is reexporting these from here a good idea?
pub use livekit_protocol::{TokenSourceRequest, TokenSourceResponse};

mod fetch_options;
mod traits;
mod minter_credentials;

pub use fetch_options::TokenSourceFetchOptions;
pub use traits::{
    TokenSourceFixed,
    TokenSourceConfigurable,
    TokenSourceFixedSynchronous,
    TokenSourceConfigurableSynchronous,
};
pub use minter_credentials::{
    MinterCredentialsSource,
    MinterCredentials,
    MinterCredentialsEnvironment,
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
    fn fetch_synchronous(&self) -> Result<TokenSourceResponse, Box<dyn Error>> {
        Ok(self.generator.apply())
    }
}




struct TokenSourceMinter<CredentialsSource: MinterCredentialsSource> {
    credentials_source: CredentialsSource,
}

impl<CS: MinterCredentialsSource> TokenSourceMinter<CS> {
    pub fn new(credentials_source: CS) -> Self {
        Self { credentials_source }
    }
}

impl<C: MinterCredentialsSource> TokenSourceConfigurableSynchronous for TokenSourceMinter<C> {
    fn fetch_synchronous(&self, options: &TokenSourceFetchOptions) -> Result<TokenSourceResponse, Box<dyn Error>> {
        let MinterCredentials { url: server_url, api_key, api_secret } = self.credentials_source.get();

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






struct TokenSourceCustomMinter<
    MintFn: Fn(access_token::AccessToken) -> Result<String, AccessTokenError>,
    Credentials: MinterCredentialsSource,
> {
    mint_fn: MintFn,
    credentials_source: Credentials,
}

impl<
    MF: Fn(access_token::AccessToken) -> Result<String, AccessTokenError>,
    CS: MinterCredentialsSource
> TokenSourceCustomMinter<MF, CS> {
    pub fn new_with_credentials(mint_fn: MF, credentials_source: CS) -> Self {
        Self { mint_fn, credentials_source }
    }
    pub fn new(mint_fn: MF) -> TokenSourceCustomMinter<MF, MinterCredentialsEnvironment> {
        TokenSourceCustomMinter::new_with_credentials(mint_fn, MinterCredentialsEnvironment::default())
    }
}


impl<
    MF: Fn(access_token::AccessToken) -> Result<String, AccessTokenError>,
    C: MinterCredentialsSource,
> TokenSourceFixedSynchronous for TokenSourceCustomMinter<MF, C> {
    fn fetch_synchronous(&self) -> Result<TokenSourceResponse, Box<dyn Error>> {
        let MinterCredentials { url: server_url, api_key, api_secret } = self.credentials_source.get();

        let participant_token = (self.mint_fn)(access_token::AccessToken::with_api_key(&api_key, &api_secret))?;

        Ok(TokenSourceResponse { server_url, participant_token })
    }
}





use reqwest::{header::HeaderMap, Method};

struct TokenSourceEndpoint {
    url: String,
    method: Method,
    headers: HeaderMap,
}

impl TokenSourceEndpoint {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.into(),
            method: Method::POST,
            headers: HeaderMap::new(),
        }
    }
}

impl TokenSourceConfigurable for TokenSourceEndpoint {
    async fn fetch(&self, options: &TokenSourceFetchOptions) -> Result<TokenSourceResponse, Box<dyn Error>> {
        let client = reqwest::Client::new();

        // FIXME: What are the best practices around implementing Into on a reference to avoid the
        // clone?
        let request_body: TokenSourceRequest = options.clone().into();

        let response = client
            .request(self.method.clone(), &self.url)
            .json(&request_body)
            .headers(self.headers.clone())
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Error generating token from endpoint {}: received {:?} / {}", self.url, response.status(), response.text().await?).into());
        }

        let response_json = response.json::<TokenSourceResponse>().await?;

        Ok(response_json)
    }
}




struct TokenSourceSandboxTokenServer(TokenSourceEndpoint);

impl TokenSourceSandboxTokenServer {
    pub fn new(sandbox_id: &str) -> Self {
        Self::new_with_base_url(sandbox_id, "https://cloud-api.livekit.io")
    }
    pub fn new_with_base_url(sandbox_id: &str, base_url: &str) -> Self {
        let mut endpoint = TokenSourceEndpoint::new(
            &format!("{base_url}/api/v2/sandbox/connection-details")
        );
        endpoint.headers.insert("X-Sandbox-ID", sandbox_id.parse().unwrap());
        Self(endpoint)
    }
}

impl TokenSourceConfigurable for TokenSourceSandboxTokenServer {
    async fn fetch(&self, options: &TokenSourceFetchOptions) -> Result<TokenSourceResponse, Box<dyn Error>> {
        self.0.fetch(options).await
    }
}




struct TokenSourceCustom<
    CustomFn: Fn(&TokenSourceFetchOptions) -> Pin<Box<dyn Future<Output = Result<TokenSourceResponse, Box<dyn Error>>>>>,
>(CustomFn);

impl<
    CustomFn: Fn(&TokenSourceFetchOptions) -> Pin<Box<dyn Future<Output = Result<TokenSourceResponse, Box<dyn Error>>>>>,
> TokenSourceCustom<CustomFn> {
    pub fn new(custom_fn: CustomFn) -> Self {
        Self(custom_fn)
    }
}

impl<
    CustomFn: Fn(&TokenSourceFetchOptions) -> Pin<Box<dyn Future<Output = Result<TokenSourceResponse, Box<dyn Error>>>>>,
> TokenSourceConfigurable for TokenSourceCustom<CustomFn> {
    async fn fetch(&self, options: &TokenSourceFetchOptions) -> Result<TokenSourceResponse, Box<dyn Error>> {
        (self.0)(options).await
    }
}




async fn test() {
    let fetch_options = TokenSourceFetchOptions::default().with_agent_name("voice ai quickstart");

    let literal = TokenSourceLiteral::new(TokenSourceResponse {
        server_url: "...".into(),
        participant_token: "...".into(),
    });
    let _ = literal.fetch().await;

    let minter = TokenSourceMinter::default();
    let _ = minter.fetch(&fetch_options).await;

    let minter_literal_credentials = TokenSourceMinter::new(MinterCredentials::new("server url", "api key", "api secret"));
    let _ = minter_literal_credentials.fetch(&fetch_options).await;

    let minter_env = TokenSourceMinter::new(MinterCredentialsEnvironment::new("SERVER_URL", "API_KEY", "API_SECRET"));
    let _ = minter_env.fetch(&fetch_options).await;

    let minter_literal_custom = TokenSourceMinter::new(|| MinterCredentials::new("server url", "api key", "api secret"));
    let _ = minter_literal_custom.fetch(&fetch_options).await;

    let custom_minter = TokenSourceCustomMinter::<_, MinterCredentialsEnvironment>::new(|access_token| {
        access_token
            .with_identity("rust-bot")
            .with_name("Rust Bot")
            .with_grants(access_token::VideoGrants {
                room_join: true,
                room: "my-room".to_string(),
                ..Default::default()
            })
            .to_jwt()
    });
    let _ = custom_minter.fetch().await;

    let endpoint = TokenSourceEndpoint::new("https://example.com/my/example/auth/endpoint");
    let _ = endpoint.fetch(&fetch_options).await;

    let endpoint = TokenSourceSandboxTokenServer::new("SANDBOX ID HERE");
    let _ = endpoint.fetch(&fetch_options).await;

    // let foo = Box::pin(async |options: &TokenSourceFetchOptions| {
    //     Ok(TokenSourceResponse::new("...", "... _options should be encoded in here ..."))
    // });

    // // TODO: custom
    // let custom = TokenSourceCustom::new(foo);
    // let _ = custom.fetch(&fetch_options).await;

    // TODO: custom
    let custom = TokenSourceCustom::new(|_options| {
        Box::pin(future::ready(
            Ok(TokenSourceResponse {
                server_url: "...".into(),
                participant_token: "... _options should be encoded in here ...".into(),
            })
        ))
    });
    let _ = custom.fetch(&fetch_options).await;
}
