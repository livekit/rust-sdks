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

use futures_util::future;
use livekit_api::access_token::{self, AccessTokenError};
use livekit_protocol::{RoomAgentDispatch, RoomConfiguration};
use serde::{Serialize, Deserialize};
use std::{collections::HashMap, error::Error, future::Future, pin::Pin};

#[derive(Clone, Default)]
pub struct TokenSourceFetchOptions {
    pub room_name: Option<String>,
    pub participant_name: Option<String>,
    pub participant_identity: Option<String>,
    pub participant_metadata: Option<String>,
    pub participant_attributes: Option<HashMap<String, String>>,

    pub agent_name: Option<String>,
    pub agent_metadata: Option<String>,
}

impl TokenSourceFetchOptions {
    pub fn with_room_name(mut self, room_name: &str) -> Self {
        self.room_name = Some(room_name.into());
        self
    }
    pub fn with_participant_name(mut self, participant_name: &str) -> Self {
        self.participant_name = Some(participant_name.into());
        self
    }
    pub fn with_participant_identity(mut self, participant_identity: &str) -> Self {
        self.participant_identity = Some(participant_identity.into());
        self
    }
    pub fn with_participant_metadata(mut self, participant_metadata: &str) -> Self {
        self.participant_metadata = Some(participant_metadata.into());
        self
    }

    fn ensure_participant_attributes_defined(&mut self) -> &mut HashMap<String, String> {
        if self.participant_attributes.is_none() {
            self.participant_attributes = Some(HashMap::new());
        };

        let Some(participant_attribute_mut) = self.participant_attributes.as_mut() else { unreachable!(); };
        participant_attribute_mut
    }

    pub fn with_participant_attribute(mut self, attribute_key: &str, attribute_value: &str) -> Self {
        self.ensure_participant_attributes_defined().insert(attribute_key.into(), attribute_value.into());
        self
    }

    pub fn with_participant_attributes(mut self, participant_attributes: HashMap<String, String>) -> Self {
        self.ensure_participant_attributes_defined().extend(participant_attributes);
        self
    }

    pub fn with_agent_name(mut self, agent_name: &str) -> Self {
        self.agent_name = Some(agent_name.into());
        self
    }
    pub fn with_agent_metadata(mut self, agent_metadata: &str) -> Self {
        self.agent_metadata = Some(agent_metadata.into());
        self
    }
}

impl Into<TokenSourceRequest> for TokenSourceFetchOptions {
    fn into(self) -> TokenSourceRequest {
        let mut agent_dispatch = RoomAgentDispatch::default();
        if let Some(agent_name) = self.agent_name {
            agent_dispatch.agent_name = agent_name;
        }
        if let Some(agent_metadata) = self.agent_metadata {
            agent_dispatch.metadata = agent_metadata;
        }

        let room_config = if agent_dispatch != RoomAgentDispatch::default() {
            let mut room_config = RoomConfiguration::default();
            room_config.agents.push(agent_dispatch);
            Some(room_config)
        } else {
            None
        };

        TokenSourceRequest {
            room_name: self.room_name,
            participant_name: self.participant_name,
            participant_identity: self.participant_identity,
            participant_metadata: self.participant_metadata,
            participant_attributes: self.participant_attributes,
            room_config,
        }
    }
}

// FIXME: use the protobuf version of this struct instead!
#[derive(Debug, Clone, Serialize)]
pub struct TokenSourceRequest {
    /// The name of the room being requested when generating credentials
    pub room_name: Option<String>,

    /// The name of the participant being requested for this client when generating credentials
    pub participant_name: Option<String>,

    /// The identity of the participant being requested for this client when generating credentials
    pub participant_identity: Option<String>,

    /// Any participant metadata being included along with the credentials generation operation
    pub participant_metadata: Option<String>,

    /// Any participant attributes being included along with the credentials generation operation
    pub participant_attributes: Option<HashMap<String, String>>,

    /// A RoomConfiguration object can be passed to request extra parameters should be included when
    /// generating connection credentials - dispatching agents, defining egress settings, etc
    /// More info: https://docs.livekit.io/home/get-started/authentication/#room-configuration
    pub room_config: Option<RoomConfiguration>,
}

// FIXME: use the protobuf version of this struct instead!
#[derive(Debug, Clone, Deserialize)]
pub struct TokenSourceResponse {
    pub server_url: String,
    pub participant_token: String,
}

impl TokenSourceResponse {
    pub fn new(server_url: &str, participant_token: &str) -> Self {
        Self { server_url: server_url.into(), participant_token: participant_token.into() }
    }
}

pub trait TokenSourceFixed {
    // FIXME: what should the error type of the result be?
    fn fetch(&self) -> impl Future<Output = Result<TokenSourceResponse, Box<dyn Error>>>;
}

pub trait TokenSourceConfigurable {
    // FIXME: what should the error type of the result be?
    fn fetch(&self, options: &TokenSourceFetchOptions) -> impl Future<Output = Result<TokenSourceResponse, Box<dyn Error>>>;
}

pub trait TokenSourceConfigurableSynchronous {
    // FIXME: what should the error type of the result be?
    fn fetch_synchronous(&self, options: &TokenSourceFetchOptions) -> Result<TokenSourceResponse, Box<dyn Error>>;
}
 
impl<T: TokenSourceConfigurableSynchronous> TokenSourceConfigurable for T {
    // FIXME: what should the error type of the result be?
    fn fetch(&self, options: &TokenSourceFetchOptions) -> impl Future<Output = Result<TokenSourceResponse, Box<dyn Error>>> {
        future::ready(self.fetch_synchronous(options))
    }
}





pub trait TokenSourceFixedSynchronous {
    // FIXME: what should the error type of the result be?
    fn fetch_synchronous(&self) -> Result<TokenSourceResponse, Box<dyn Error>>;
}
 
impl<T: TokenSourceFixedSynchronous> TokenSourceFixed for T {
    // FIXME: what should the error type of the result be?
    fn fetch(&self) -> impl Future<Output = Result<TokenSourceResponse, Box<dyn Error>>> {
        future::ready(self.fetch_synchronous())
    }
}



pub trait TokenLiteralGenerator {
    fn apply(&self) -> TokenSourceResponse;
}

impl TokenLiteralGenerator for TokenSourceResponse {
    fn apply(&self) -> TokenSourceResponse {
        self.clone()
    }
}

impl<F: Fn() -> TokenSourceResponse> TokenLiteralGenerator for F {
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




trait MinterCredentials {
    fn get(&self) -> (String, String, String);
}

struct MinterCredentialsLiteral(String, String, String);

impl MinterCredentialsLiteral {
    pub fn new(server_url: &str, api_key: &str, api_secret: &str) -> Self {
        Self(server_url.into(), api_key.into(), api_secret.into())
    }
}

impl MinterCredentials for MinterCredentialsLiteral {
    fn get(&self) -> (String, String, String) {
        (self.0.clone(), self.1.clone(), self.2.clone())
    }
}

// FIXME: maybe add dotenv source too? Or have this look there too?
struct MinterCredentialsEnvironment(String, String, String);

impl MinterCredentialsEnvironment {
    pub fn new(url_variable: &str, api_key_variable: &str, api_secret_variable: &str) -> Self {
        Self(url_variable.into(), api_key_variable.into(), api_secret_variable.into())
    }
}

impl MinterCredentials for MinterCredentialsEnvironment {
    fn get(&self) -> (String, String, String) {
        let (url_variable, api_key_variable, api_secret_variable) = (&self.0, &self.1, &self.2);
        let url = std::env::var(url_variable).expect(format!("{url_variable} is not set").as_str());
        let api_key = std::env::var(api_key_variable).expect(format!("{api_key_variable} is not set").as_str());
        let api_secret = std::env::var(api_secret_variable).expect(format!("{api_secret_variable} is not set").as_str());
        (url, api_key, api_secret)
    }
}

impl Default for MinterCredentialsEnvironment {
    fn default() -> Self {
        Self("LIVEKIT_URL".into(), "LIVEKIT_API_KEY".into(), "LIVEKIT_API_SECRET".into())
    }
}

impl<F: Fn() -> (String, String, String)> MinterCredentials for F {
    fn get(&self) -> (String, String, String) {
        self()
    }
}


struct TokenSourceMinter<Credentials: MinterCredentials> {
    credentials: Credentials,
}

impl<C: MinterCredentials> TokenSourceMinter<C> {
    pub fn new(credentials: C) -> Self {
        Self { credentials }
    }
}

impl<C: MinterCredentials> TokenSourceConfigurableSynchronous for TokenSourceMinter<C> {
    fn fetch_synchronous(&self, options: &TokenSourceFetchOptions) -> Result<TokenSourceResponse, Box<dyn Error>> {
        let (server_url, api_key, api_secret) = self.credentials.get();

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

        Ok(TokenSourceResponse::new(server_url.as_str(), participant_token.as_str()))
    }
}

impl Default for TokenSourceMinter<MinterCredentialsEnvironment> {
    fn default() -> Self {
        Self::new(MinterCredentialsEnvironment::default())
    }
}






struct TokenSourceCustomMinter<
    MintFn: Fn(access_token::AccessToken) -> Result<String, AccessTokenError>,
    Credentials: MinterCredentials,
> {
    mint_fn: MintFn,
    credentials: Credentials,
}

impl<
    MF: Fn(access_token::AccessToken) -> Result<String, AccessTokenError>,
    C: MinterCredentials
> TokenSourceCustomMinter<MF, C> {
    pub fn new_with_credentials(mint_fn: MF, credentials: C) -> Self {
        Self { mint_fn, credentials }
    }
    pub fn new(mint_fn: MF) -> TokenSourceCustomMinter<MF, MinterCredentialsEnvironment> {
        TokenSourceCustomMinter::new_with_credentials(mint_fn, MinterCredentialsEnvironment::default())
    }
}


impl<
    MF: Fn(access_token::AccessToken) -> Result<String, AccessTokenError>,
    C: MinterCredentials,
> TokenSourceFixedSynchronous for TokenSourceCustomMinter<MF, C> {
    fn fetch_synchronous(&self) -> Result<TokenSourceResponse, Box<dyn Error>> {
        let (server_url, api_key, api_secret) = self.credentials.get();

        let participant_token = (self.mint_fn)(access_token::AccessToken::with_api_key(&api_key, &api_secret))?;

        Ok(TokenSourceResponse::new(server_url.as_str(), participant_token.as_str()))
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

        Ok(TokenSourceResponse::new(&response_json.server_url, &response_json.participant_token))
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
    CustomFn: AsyncFn(&TokenSourceFetchOptions) -> Result<TokenSourceResponse, Box<dyn Error>>,
>(CustomFn);

impl<
    CustomFn: AsyncFn(&TokenSourceFetchOptions) -> Result<TokenSourceResponse, Box<dyn Error>>,
> TokenSourceCustom<CustomFn> {
    pub fn new(custom_fn: CustomFn) -> Self {
        Self(custom_fn)
    }
}

impl<
    CustomFn: AsyncFn(&TokenSourceFetchOptions) -> Result<TokenSourceResponse, Box<dyn Error>>,
> TokenSourceConfigurable for TokenSourceCustom<CustomFn> {
    async fn fetch(&self, options: &TokenSourceFetchOptions) -> Result<TokenSourceResponse, Box<dyn Error>> {
        (self.0)(options).await
    }
}




async fn test() {
    let fetch_options = TokenSourceFetchOptions::default().with_agent_name("voice ai quickstart");

    let literal = TokenSourceLiteral::new(TokenSourceResponse::new("...", "..."));
    let _ = literal.fetch().await;

    let minter = TokenSourceMinter::default();
    let _ = minter.fetch(&fetch_options).await;

    let minter_literal_credentials = TokenSourceMinter::new(MinterCredentialsLiteral::new("server url", "api key", "api secret"));
    let _ = minter.fetch(&fetch_options).await;

    let minter_literal_custom = TokenSourceMinter::new(|| ("server url".into(), "api key".into(), "api secret".into()));
    let _ = minter.fetch(&fetch_options).await;

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

    // TODO: custom
    let custom = TokenSourceCustom::new(async |_options| {
        Ok(TokenSourceResponse::new("...", "... _options should be encoded in here ..."))
    });
    let _ = custom.fetch(&fetch_options).await;
}
