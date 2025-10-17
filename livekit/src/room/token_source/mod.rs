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
use parking_lot::RwLock;
use std::{future::Future, pin::Pin, sync::Arc};

mod error;
mod fetch_options;
mod minter_credentials;
mod request_response;
mod traits;

pub use error::{TokenSourceError, TokenSourceResult};
pub use fetch_options::TokenSourceFetchOptions;
pub use minter_credentials::{
    MinterCredentials, MinterCredentialsEnvironment, MinterCredentialsSource,
};
pub use request_response::{TokenSourceRequest, TokenSourceResponse};
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







trait TokenResponseCacheValue {}

impl TokenResponseCacheValue for TokenSourceResponse {}
impl TokenResponseCacheValue for (TokenSourceFetchOptions, TokenSourceResponse) {}

/// Represents a mechanism by which token responses can be cached
///
/// When used with a TokenSourceFixed, `Value` is `TokenSourceResponse`
/// When used with a TokenSourceConfigurable, `Value` is `(TokenSourceFetchOptions, TokenSourceResponse)`
trait TokenResponseCache<Value: TokenResponseCacheValue> {
    fn get(&self) -> Option<&Value>;
    fn set(&mut self, value: Value);
    fn clear(&mut self);
}

/// In-memory implementation of [TokenResponseCache]
struct TokenResponseInMemoryCache<Value: TokenResponseCacheValue>(Option<Value>);
impl<Value: TokenResponseCacheValue> TokenResponseInMemoryCache<Value> {
    pub fn new() -> Self {
        Self(None)
    }
}

impl<Value: TokenResponseCacheValue> TokenResponseCache<Value> for TokenResponseInMemoryCache<Value> {
    fn get(&self) -> Option<&Value> {
        self.0.as_ref()
    }
    fn set(&mut self, value: Value) {
        self.0 = Some(value);
    }
    fn clear(&mut self) {
        self.0 = None;
    }
}








trait TokenSourceFixedCached {
    fn get_response_cache(&self) -> Arc<RwLock<impl TokenResponseCache<TokenSourceResponse>>>;

    async fn update(&self) -> TokenSourceResult<TokenSourceResponse>;

    async fn fetch_cached(&self) -> TokenSourceResult<TokenSourceResponse> {
        let cache = self.get_response_cache();

        let cached_response_to_return = {
            let cache_read = cache.read();
            let cached_value = cache_read.get();

            if let Some(cached_response) = cached_value {
                if access_token::is_token_valid(&cached_response.participant_token)? {
                    Some(cached_response.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(cached_response) = cached_response_to_return {
            Ok(cached_response)
        } else {
            let response = self.update().await?;
            cache.write().set(response.clone());
            Ok(response)
        }
    }
}

trait TokenSourceConfigurableCached {
    fn get_response_cache(&self) -> Arc<RwLock<impl TokenResponseCache<(TokenSourceFetchOptions, TokenSourceResponse)>>>;

    async fn update(&self, options: &TokenSourceFetchOptions) -> TokenSourceResult<TokenSourceResponse>;

    async fn fetch_cached(
        &self,
        options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse> {
        let cache = self.get_response_cache();

        let cached_response_to_return = {
            let cache_read = cache.read();
            let cached_value = cache_read.get();

            if let Some((cached_options, cached_response)) = cached_value {
                if options == cached_options && access_token::is_token_valid(&cached_response.participant_token)? {
                    Some(cached_response.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(cached_response) = cached_response_to_return {
            Ok(cached_response)
        } else {
            let response = self.update(options).await?;
            cache.write().set((options.clone(), response.clone()));
            Ok(response)
        }
    }
}

// FIXME: Why doesn't this work?
// impl<T: TokenSourceConfigurableCached> TokenSourceConfigurable for T {
//     async fn fetch(
//         &self,
//         options: &TokenSourceFetchOptions,
//     ) -> TokenSourceResult<TokenSourceResponse> {
//         self.fetch_cached(options).await
//     }
// }






trait TokenSourceCacheType {}

struct TokenSourceCacheConfigurable<T: TokenSourceConfigurable>(T);
impl<T: TokenSourceConfigurable> TokenSourceCacheType for TokenSourceCacheConfigurable<T> {}

struct TokenSourceCacheFixed<T: TokenSourceFixed>(T);
impl<T: TokenSourceFixed> TokenSourceCacheType for TokenSourceCacheFixed<T> {}

/// A conmposable TokenSource which can wrap either a [TokenSourceFixed] or a [TokenSourceConfigurable] and
/// caches the intermediate value in a [TokenResponseCache].
struct TokenSourceCache<Type: TokenSourceCacheType, Value: TokenResponseCacheValue, Cache: TokenResponseCache<Value>> {
    inner: Type,
    cache: Arc<RwLock<Cache>>,
    _v: Value, // FIXME: how do I remove this? `Value` needs to be used in here or I get an error.
}

impl<Inner: TokenSourceConfigurable> TokenSourceCache<
    TokenSourceCacheConfigurable<Inner>,
    (TokenSourceFetchOptions, TokenSourceResponse),
    TokenResponseInMemoryCache<(TokenSourceFetchOptions, TokenSourceResponse)>
> {
    // FIXME: Is there some way I can make this `new` without requiring something like the below?
    // TokenSourceCache::<TokenSourceCacheConfigurable<_>, _, _>::new(...)
    fn new_configurable(inner_token_source: Inner) -> Self {
        TokenSourceCache::new_configurable_with_cache(inner_token_source, TokenResponseInMemoryCache::new())
    }
}

impl<Inner: TokenSourceFixed> TokenSourceCache<
    TokenSourceCacheFixed<Inner>,
    TokenSourceResponse,
    TokenResponseInMemoryCache<TokenSourceResponse>
> {
    // FIXME: Is there some way I can make this `new` without requiring something like the below?
    // TokenSourceCache::<TokenSourceCacheFixed<_>, _, _>::new(...)
    fn new_fixed(inner_token_source: Inner) -> Self {
        TokenSourceCache::new_fixed_with_cache(inner_token_source, TokenResponseInMemoryCache::new())
    }
}

impl<
    Inner: TokenSourceConfigurable,
    Cache: TokenResponseCache<(TokenSourceFetchOptions, TokenSourceResponse)>
> TokenSourceCache<
    TokenSourceCacheConfigurable<Inner>,
    (TokenSourceFetchOptions, TokenSourceResponse),
    Cache
> {
    fn new_configurable_with_cache(inner_token_source: Inner, token_cache: Cache) -> Self {
        Self {
            inner: TokenSourceCacheConfigurable(inner_token_source),
            cache: Arc::new(RwLock::new(token_cache)),

            // FIXME: remove this!
            _v: (TokenSourceFetchOptions::default(), TokenSourceResponse { server_url: "".into(), participant_token: "".into() }),
        }
    }
}

impl<
    Inner: TokenSourceFixed,
    Cache: TokenResponseCache<TokenSourceResponse>
> TokenSourceCache<
    TokenSourceCacheFixed<Inner>,
    TokenSourceResponse,
    Cache
> {
    fn new_fixed_with_cache(inner_token_source: Inner, token_cache: Cache) -> Self {
        Self {
            inner: TokenSourceCacheFixed(inner_token_source),
            cache: Arc::new(RwLock::new(token_cache)),

            // FIXME: remove this!
            _v: TokenSourceResponse { server_url: "".into(), participant_token: "".into() },
        }
    }
}


impl<
    Inner: TokenSourceConfigurable,
    Cache: TokenResponseCache<(TokenSourceFetchOptions, TokenSourceResponse)>
> TokenSourceConfigurableCached for TokenSourceCache<
    TokenSourceCacheConfigurable<Inner>,
    (TokenSourceFetchOptions, TokenSourceResponse),
    Cache,
> {
    fn get_response_cache(&self) -> Arc<RwLock<impl TokenResponseCache<(TokenSourceFetchOptions, TokenSourceResponse)>>> {
        self.cache.clone()
    }
    async fn update(&self, options: &TokenSourceFetchOptions) -> TokenSourceResult<TokenSourceResponse> {
        self.inner.0.fetch(options).await
    }
}

impl<
    Inner: TokenSourceFixed,
    Cache: TokenResponseCache<TokenSourceResponse>
> TokenSourceFixedCached for TokenSourceCache<
    TokenSourceCacheFixed<Inner>,
    TokenSourceResponse,
    Cache,
> {
    fn get_response_cache(&self) -> Arc<RwLock<impl TokenResponseCache<TokenSourceResponse>>> {
        self.cache.clone()
    }
    async fn update(&self) -> TokenSourceResult<TokenSourceResponse> {
        self.inner.0.fetch().await
    }
}




impl<
    Inner: TokenSourceConfigurable,
    Cache: TokenResponseCache<(TokenSourceFetchOptions, TokenSourceResponse)>
> TokenSourceConfigurable for TokenSourceCache<
    TokenSourceCacheConfigurable<Inner>,
    (TokenSourceFetchOptions, TokenSourceResponse),
    Cache,
> {
    async fn fetch(&self, options: &TokenSourceFetchOptions) -> TokenSourceResult<TokenSourceResponse> {
        self.fetch_cached(options).await
    }
}

impl<
    Inner: TokenSourceFixed,
    Cache: TokenResponseCache<TokenSourceResponse>
> TokenSourceFixed for TokenSourceCache<
    TokenSourceCacheFixed<Inner>,
    TokenSourceResponse,
    Cache,
> {
    async fn fetch(&self) -> TokenSourceResult<TokenSourceResponse> {
        self.fetch_cached().await
    }
}





fn test() {
    let a = TokenSourceCache::new_configurable(TokenSourceMinter::default());

    let minter = TokenSourceMinter::default();
    let cache = TokenResponseInMemoryCache::new();
    let b = TokenSourceCache::new_configurable_with_cache(minter, cache);
}
