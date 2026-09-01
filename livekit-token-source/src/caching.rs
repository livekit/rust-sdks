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

use std::sync::Mutex;

use async_trait::async_trait;

use crate::request::TokenSourceFetchOptions;
use crate::response::{TokenSourceResponse, TokenSourceResult};
use crate::token_source::TokenSourceConfigurable;

/// Persistence backend for [`TokenSourceCached`].
///
/// Implement this trait to keep credentials in a custom location, e.g. the
/// platform keychain or a database. The default is the process-lifetime
/// [`TokenSourceInMemoryStore`].
#[async_trait]
pub trait TokenSourceStore: Send + Sync {
    /// Stores the given credentials, replacing any stored previously.
    async fn store(&self, options: TokenSourceFetchOptions, response: TokenSourceResponse);

    /// Returns the stored credentials, or `None` if nothing is stored.
    async fn retrieve(&self) -> Option<(TokenSourceFetchOptions, TokenSourceResponse)>;

    /// Removes the stored credentials.
    async fn clear(&self);
}

/// The default [`TokenSourceStore`]: keeps credentials in memory, losing them
/// when the process exits.
#[derive(Default)]
pub struct TokenSourceInMemoryStore {
    cached: Mutex<Option<(TokenSourceFetchOptions, TokenSourceResponse)>>,
}

#[async_trait]
impl TokenSourceStore for TokenSourceInMemoryStore {
    // Lock poisoning is recovered from rather than propagated: the guarded
    // data is a plain `Option`, left valid even if a holder panicked mid-way.
    async fn store(&self, options: TokenSourceFetchOptions, response: TokenSourceResponse) {
        *self.cached.lock().unwrap_or_else(|e| e.into_inner()) = Some((options, response));
    }

    async fn retrieve(&self) -> Option<(TokenSourceFetchOptions, TokenSourceResponse)> {
        self.cached.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    async fn clear(&self) {
        *self.cached.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

type Validator = Box<dyn Fn(&TokenSourceFetchOptions, &TokenSourceResponse) -> bool + Send + Sync>;

/// The return type of [`TokenSourceConfigurable::cached`]: wraps another token
/// source and stores the last fetched credentials, serving them for repeat
/// fetches with equal options for as long as they stay valid.
///
/// Credentials are kept in a [`TokenSourceStore`] (by default in memory, see
/// [`TokenSourceCached::with_store`]) and are considered valid as long as a
/// validator accepts them (by default until the token expires, see
/// [`TokenSourceCached::with_validator`]).
///
/// Concurrent fetches that miss the cache are not deduplicated; each hits the
/// underlying source and the last response wins.
pub struct TokenSourceCached<S> {
    source: S,
    store: Box<dyn TokenSourceStore>,
    validator: Validator,
}

impl<S> TokenSourceCached<S> {
    pub(crate) fn new(source: S) -> Self {
        Self {
            source,
            store: Box::new(TokenSourceInMemoryStore::default()),
            validator: Box::new(|_, response| response.has_valid_token()),
        }
    }

    /// Replaces the default in-memory store with a custom [`TokenSourceStore`],
    /// e.g. one persisting credentials to the platform keychain or a database.
    pub fn with_store(mut self, store: impl TokenSourceStore + 'static) -> Self {
        self.store = Box::new(store);
        self
    }

    /// Replaces the default validator deciding whether stored credentials are
    /// still valid or must be refetched. The default checks that the token has
    /// not expired ([`TokenSourceResponse::has_valid_token`]).
    ///
    /// Pass the closure inline (or annotate its parameter types); binding it
    /// to a variable first can fail closure type inference.
    pub fn with_validator<F>(mut self, validator: F) -> Self
    where
        F: Fn(&TokenSourceFetchOptions, &TokenSourceResponse) -> bool + Send + Sync + 'static,
    {
        self.validator = Box::new(validator);
        self
    }

    /// Removes the stored credentials, forcing the next fetch to hit the
    /// underlying source.
    ///
    /// A fetch already in flight is unaffected: it still resolves and stores
    /// its response afterwards, repopulating the cache (last writer wins).
    pub async fn invalidate(&self) {
        self.store.clear().await;
    }

    /// Returns the last stored response, if any — without checking its
    /// validity or which options it was fetched with, so it may be expired.
    pub async fn cached_response(&self) -> Option<TokenSourceResponse> {
        self.store.retrieve().await.map(|(_, response)| response)
    }
}

#[async_trait]
impl<S: TokenSourceConfigurable + Send + Sync> TokenSourceConfigurable for TokenSourceCached<S> {
    async fn fetch(
        &self,
        options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse> {
        if let Some((cached_options, cached_response)) = self.store.retrieve().await {
            if cached_options == *options && (self.validator)(&cached_options, &cached_response) {
                return Ok(cached_response);
            }
        }

        let response = self.source.fetch(options).await?;
        self.store.store(options.clone(), response.clone()).await;
        Ok(response)
    }
}
