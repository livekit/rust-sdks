use livekit_api::access_token;
use parking_lot::RwLock;
use std::sync::Arc;

use crate::token_source::{
    TokenResponseCache, TokenResponseCacheValue, TokenResponseInMemoryCache,
    TokenSourceConfigurable, TokenSourceFetchOptions, TokenSourceFixed, TokenSourceResponse,
    TokenSourceResult,
};

pub trait TokenSourceFixedCached {
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

pub trait TokenSourceConfigurableCached {
    fn get_response_cache(
        &self,
    ) -> Arc<RwLock<impl TokenResponseCache<(TokenSourceFetchOptions, TokenSourceResponse)>>>;

    async fn update(
        &self,
        options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse>;

    async fn fetch_cached(
        &self,
        options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse> {
        let cache = self.get_response_cache();

        let cached_response_to_return = {
            let cache_read = cache.read();
            let cached_value = cache_read.get();

            if let Some((cached_options, cached_response)) = cached_value {
                if options == cached_options
                    && access_token::is_token_valid(&cached_response.participant_token)?
                {
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

trait CacheType {}

pub struct CacheConfigurable<T: TokenSourceConfigurable>(T);
impl<T: TokenSourceConfigurable> CacheType for CacheConfigurable<T> {}

pub struct CacheFixed<T: TokenSourceFixed>(T);
impl<T: TokenSourceFixed> CacheType for CacheFixed<T> {}

/// A conmposable TokenSource which can wrap either a [TokenSourceFixed] or a [TokenSourceConfigurable] and
/// caches the intermediate value in a [TokenResponseCache].
pub struct TokenSourceCache<
    Type: CacheType,
    Value: TokenResponseCacheValue,
    Cache: TokenResponseCache<Value>,
> {
    inner: Type,
    cache: Arc<RwLock<Cache>>,
    _v: Value, // FIXME: how do I remove this? `Value` needs to be used in here or I get an error.
}

impl<Inner: TokenSourceConfigurable>
    TokenSourceCache<
        CacheConfigurable<Inner>,
        (TokenSourceFetchOptions, TokenSourceResponse),
        TokenResponseInMemoryCache<(TokenSourceFetchOptions, TokenSourceResponse)>,
    >
{
    // FIXME: Is there some way I can make this `new` without requiring something like the below?
    // TokenSourceCache::<TokenSourceCacheConfigurable<_>, _, _>::new(...)
    fn new_configurable(inner_token_source: Inner) -> Self {
        TokenSourceCache::new_configurable_with_cache(
            inner_token_source,
            TokenResponseInMemoryCache::new(),
        )
    }
}

impl<Inner: TokenSourceFixed>
    TokenSourceCache<
        CacheFixed<Inner>,
        TokenSourceResponse,
        TokenResponseInMemoryCache<TokenSourceResponse>,
    >
{
    // FIXME: Is there some way I can make this `new` without requiring something like the below?
    // TokenSourceCache::<TokenSourceCacheFixed<_>, _, _>::new(...)
    fn new_fixed(inner_token_source: Inner) -> Self {
        TokenSourceCache::new_fixed_with_cache(
            inner_token_source,
            TokenResponseInMemoryCache::new(),
        )
    }
}

impl<
        Inner: TokenSourceConfigurable,
        Cache: TokenResponseCache<(TokenSourceFetchOptions, TokenSourceResponse)>,
    >
    TokenSourceCache<
        CacheConfigurable<Inner>,
        (TokenSourceFetchOptions, TokenSourceResponse),
        Cache,
    >
{
    fn new_configurable_with_cache(inner_token_source: Inner, token_cache: Cache) -> Self {
        Self {
            inner: CacheConfigurable(inner_token_source),
            cache: Arc::new(RwLock::new(token_cache)),

            // FIXME: remove this!
            _v: (
                TokenSourceFetchOptions::default(),
                TokenSourceResponse { server_url: "".into(), participant_token: "".into() },
            ),
        }
    }
}

impl<Inner: TokenSourceFixed, Cache: TokenResponseCache<TokenSourceResponse>>
    TokenSourceCache<CacheFixed<Inner>, TokenSourceResponse, Cache>
{
    fn new_fixed_with_cache(inner_token_source: Inner, token_cache: Cache) -> Self {
        Self {
            inner: CacheFixed(inner_token_source),
            cache: Arc::new(RwLock::new(token_cache)),

            // FIXME: remove this!
            _v: TokenSourceResponse { server_url: "".into(), participant_token: "".into() },
        }
    }
}

impl<
        Inner: TokenSourceConfigurable,
        Cache: TokenResponseCache<(TokenSourceFetchOptions, TokenSourceResponse)>,
    > TokenSourceConfigurableCached
    for TokenSourceCache<
        CacheConfigurable<Inner>,
        (TokenSourceFetchOptions, TokenSourceResponse),
        Cache,
    >
{
    fn get_response_cache(
        &self,
    ) -> Arc<RwLock<impl TokenResponseCache<(TokenSourceFetchOptions, TokenSourceResponse)>>> {
        self.cache.clone()
    }
    async fn update(
        &self,
        options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse> {
        self.inner.0.fetch(options).await
    }
}

impl<Inner: TokenSourceFixed, Cache: TokenResponseCache<TokenSourceResponse>> TokenSourceFixedCached
    for TokenSourceCache<CacheFixed<Inner>, TokenSourceResponse, Cache>
{
    fn get_response_cache(&self) -> Arc<RwLock<impl TokenResponseCache<TokenSourceResponse>>> {
        self.cache.clone()
    }
    async fn update(&self) -> TokenSourceResult<TokenSourceResponse> {
        self.inner.0.fetch().await
    }
}

impl<
        Inner: TokenSourceConfigurable,
        Cache: TokenResponseCache<(TokenSourceFetchOptions, TokenSourceResponse)>,
    > TokenSourceConfigurable
    for TokenSourceCache<
        CacheConfigurable<Inner>,
        (TokenSourceFetchOptions, TokenSourceResponse),
        Cache,
    >
{
    async fn fetch(
        &self,
        options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse> {
        self.fetch_cached(options).await
    }
}

impl<Inner: TokenSourceFixed, Cache: TokenResponseCache<TokenSourceResponse>> TokenSourceFixed
    for TokenSourceCache<CacheFixed<Inner>, TokenSourceResponse, Cache>
{
    async fn fetch(&self) -> TokenSourceResult<TokenSourceResponse> {
        self.fetch_cached().await
    }
}
