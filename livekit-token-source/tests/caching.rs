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

//! Behavior tests for `TokenSourceCached`, using a counting mock source
//! instead of HTTP (the mock HTTP registry in `mock_http.rs` is process-wide
//! and not needed here).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use base64::Engine as _;
use livekit_token_source::{
    TokenSourceConfigurable, TokenSourceError, TokenSourceFetchOptions, TokenSourceInMemoryStore,
    TokenSourceResponse, TokenSourceResult, TokenSourceStore,
};

/// Builds an unsigned JWT with the given claims payload.
fn jwt(claims: serde_json::Value) -> String {
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    format!(
        "{}.{}.{}",
        engine.encode(r#"{"alg":"HS256","typ":"JWT"}"#),
        engine.encode(claims.to_string()),
        engine.encode("signature")
    )
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn valid_jwt() -> String {
    jwt(serde_json::json!({ "exp": now_secs() + 3600 }))
}

fn expired_jwt() -> String {
    jwt(serde_json::json!({ "exp": now_secs() - 3600 }))
}

fn response(token: impl Into<String>) -> TokenSourceResponse {
    TokenSourceResponse {
        server_url: "wss://example.livekit.cloud".into(),
        participant_token: token.into(),
    }
}

/// A mock source counting its fetches; returns the shared `next` response,
/// or a server error when `next` is `None`.
struct CountingSource {
    calls: Arc<AtomicUsize>,
    next: Arc<Mutex<Option<TokenSourceResponse>>>,
}

impl CountingSource {
    fn new(
        token: impl Into<String>,
    ) -> (Self, Arc<AtomicUsize>, Arc<Mutex<Option<TokenSourceResponse>>>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let next = Arc::new(Mutex::new(Some(response(token))));
        (Self { calls: calls.clone(), next: next.clone() }, calls, next)
    }
}

#[async_trait]
impl TokenSourceConfigurable for CountingSource {
    async fn fetch(
        &self,
        _options: &TokenSourceFetchOptions,
    ) -> TokenSourceResult<TokenSourceResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.next.lock().unwrap().clone() {
            Some(response) => Ok(response),
            None => Err(TokenSourceError::Server { status: 500, body: "mock error".into() }),
        }
    }
}

#[tokio::test]
async fn cache_hit_serves_from_cache() {
    let (source, calls, _) = CountingSource::new(valid_jwt());
    let cached = source.cached();
    let options = TokenSourceFetchOptions::new().with_room_name("room");

    let first = cached.fetch(&options).await.unwrap();
    let second = cached.fetch(&options).await.unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(first, second);
}

#[tokio::test]
async fn different_options_bypass_cache() {
    let (source, calls, _) = CountingSource::new(valid_jwt());
    let cached = source.cached();
    let options_a = TokenSourceFetchOptions::new().with_room_name("room-a");
    let options_b = TokenSourceFetchOptions::new().with_room_name("room-b");

    cached.fetch(&options_a).await.unwrap();
    cached.fetch(&options_b).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    // The entry for room-b replaced room-a's, so room-b is now a hit.
    cached.fetch(&options_b).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn expired_token_triggers_refetch() {
    let (source, calls, next) = CountingSource::new(expired_jwt());
    let cached = source.cached();
    let options = TokenSourceFetchOptions::new().with_room_name("room");

    cached.fetch(&options).await.unwrap();
    cached.fetch(&options).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    // Once the source hands out a valid token, fetches become hits again.
    *next.lock().unwrap() = Some(response(valid_jwt()));
    cached.fetch(&options).await.unwrap();
    cached.fetch(&options).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn default_validator_rejects_opaque_token() {
    let (source, calls, _) = CountingSource::new("opaque-token");
    let cached = source.cached();
    let options = TokenSourceFetchOptions::new();

    cached.fetch(&options).await.unwrap();
    cached.fetch(&options).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn custom_validator_caches_opaque_tokens() {
    let (source, calls, _) = CountingSource::new("opaque-token");
    let cached = source.cached().with_validator(|_, _| true);
    let options = TokenSourceFetchOptions::new();

    cached.fetch(&options).await.unwrap();
    cached.fetch(&options).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn custom_validator_can_disable_caching() {
    let (source, calls, _) = CountingSource::new(valid_jwt());
    let cached = source.cached().with_validator(|_, _| false);
    let options = TokenSourceFetchOptions::new();

    cached.fetch(&options).await.unwrap();
    cached.fetch(&options).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn invalidate_clears_cache() {
    let (source, calls, _) = CountingSource::new(valid_jwt());
    let cached = source.cached();
    let options = TokenSourceFetchOptions::new();

    cached.fetch(&options).await.unwrap();
    cached.invalidate().await;
    assert!(cached.cached_response().await.is_none());

    cached.fetch(&options).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn cached_response_reflects_store() {
    let (source, _, _) = CountingSource::new(expired_jwt());
    let cached = source.cached();
    let options = TokenSourceFetchOptions::new();

    assert!(cached.cached_response().await.is_none());

    let fetched = cached.fetch(&options).await.unwrap();
    // The last stored response is returned even though it is expired:
    // cached_response never validates.
    assert_eq!(cached.cached_response().await, Some(fetched));
}

/// A store delegating to the in-memory store while recording its calls.
struct RecordingStore {
    inner: TokenSourceInMemoryStore,
    events: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl TokenSourceStore for RecordingStore {
    async fn store(&self, options: TokenSourceFetchOptions, response: TokenSourceResponse) {
        self.events.lock().unwrap().push(format!("store:{}", response.participant_token));
        self.inner.store(options, response).await;
    }

    async fn retrieve(&self) -> Option<(TokenSourceFetchOptions, TokenSourceResponse)> {
        self.events.lock().unwrap().push("retrieve".into());
        self.inner.retrieve().await
    }

    async fn clear(&self) {
        self.events.lock().unwrap().push("clear".into());
        self.inner.clear().await;
    }
}

#[tokio::test]
async fn custom_store_is_used() {
    let token = valid_jwt();
    let (source, calls, _) = CountingSource::new(token.clone());
    let events = Arc::new(Mutex::new(Vec::new()));
    let cached = source.cached().with_store(RecordingStore {
        inner: TokenSourceInMemoryStore::default(),
        events: events.clone(),
    });
    let options = TokenSourceFetchOptions::new();

    cached.fetch(&options).await.unwrap();
    cached.fetch(&options).await.unwrap();

    // First fetch misses (retrieve, then store of the fetched token); the
    // second is served from the custom store.
    let expected = vec!["retrieve".to_string(), format!("store:{token}"), "retrieve".to_string()];
    assert_eq!(*events.lock().unwrap(), expected);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

/// A store that never retains anything.
struct NullStore;

#[async_trait]
impl TokenSourceStore for NullStore {
    async fn store(&self, _options: TokenSourceFetchOptions, _response: TokenSourceResponse) {}

    async fn retrieve(&self) -> Option<(TokenSourceFetchOptions, TokenSourceResponse)> {
        None
    }

    async fn clear(&self) {}
}

#[tokio::test]
async fn null_store_forces_fetch_every_time() {
    let (source, calls, _) = CountingSource::new(valid_jwt());
    let cached = source.cached().with_store(NullStore);
    let options = TokenSourceFetchOptions::new();

    cached.fetch(&options).await.unwrap();
    cached.fetch(&options).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn error_propagates_and_cache_is_unchanged() {
    let token = valid_jwt();
    let (source, calls, next) = CountingSource::new(token.clone());
    let cached = source.cached();
    let options_a = TokenSourceFetchOptions::new().with_room_name("room-a");
    let options_b = TokenSourceFetchOptions::new().with_room_name("room-b");

    cached.fetch(&options_a).await.unwrap();

    *next.lock().unwrap() = None;
    let error = cached.fetch(&options_b).await.unwrap_err();
    assert!(matches!(error, TokenSourceError::Server { status: 500, .. }));

    // The failed fetch left room-a's credentials in place, so fetching
    // room-a again is still a hit.
    assert_eq!(cached.cached_response().await.map(|r| r.participant_token), Some(token));
    cached.fetch(&options_a).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
