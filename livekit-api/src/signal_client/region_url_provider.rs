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

use std::{
    collections::HashMap,
    error::Error as StdError,
    sync::{Arc, OnceLock},
    time::Duration,
};

use parking_lot::Mutex;
use tokio::sync::Mutex as AsyncMutex;

use crate::region::{is_cloud_host, parse_max_age, Cached, RegionCache, RegionsResponse};

use super::{SignalError, SignalResult, REGION_FETCH_TIMEOUT};

/// Process-wide region cache for the signaling path. Persisting it here (rather
/// than on a per-connection object) means it survives across reconnect attempts
/// — each of which rebuilds the SignalClient — so the reconnect loop does not
/// re-pay the region fetch on every attempt. The caching logic lives in
/// [`RegionCache`]; this owns the signaling instance (which stores `wss://` URLs).
fn region_cache() -> &'static RegionCache {
    static CACHE: OnceLock<RegionCache> = OnceLock::new();
    CACHE.get_or_init(|| RegionCache::new(RegionCache::DEFAULT_TTL))
}

/// Returns the per-host fetch lock, creating it on first use. Held across the
/// network request so only one fetch per host runs at a time — concurrent cache
/// misses for the same host collapse into a single request (single-flight) —
/// after which the waiters pick up the result from the cache.
fn fetch_lock(host: &str) -> Arc<AsyncMutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> = OnceLock::new();
    LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .entry(host.to_string())
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

fn region_host(url: &str) -> SignalResult<String> {
    let parsed = url::Url::parse(url).map_err(|err| SignalError::UrlParse(err.to_string()))?;
    parsed
        .host_str()
        .map(|h| h.to_string())
        .ok_or_else(|| SignalError::UrlParse("invalid hostname".into()))
}

/// Converts an error into a string that includes the full error chain.
/// This is important for debugging TLS errors, where the root cause
/// (e.g., "invalid peer certificate: UnknownIssuer") is often buried
/// in the source chain.
fn error_with_chain(err: &dyn StdError) -> String {
    let mut source = err.source();

    std::iter::once(err.to_string())
        .chain(std::iter::from_fn(move || {
            let err = source?;
            source = err.source();
            Some(err.to_string())
        }))
        .collect::<Vec<_>>()
        .join(": ")
}

pub struct RegionUrlProvider;

impl RegionUrlProvider {
    /// Fetch the ordered list of region signalling URLs for a LiveKit Cloud
    /// host. Non-cloud (direct / self-hosted) URLs have no regions, so this
    /// returns an empty list. Successful results are cached per host for the
    /// server's `Cache-Control: max-age` (or [`RegionCache::DEFAULT_TTL`] when
    /// absent); failures are never cached. Once an entry goes stale a re-fetch
    /// is attempted, but if it fails the stale entry is returned as a fallback
    /// rather than surfacing the error. Concurrent calls for the same host are
    /// de-duplicated: only one fetch runs at a time and the rest reuse its
    /// result.
    pub async fn fetch_region_urls(url: &str, token: &str) -> SignalResult<Vec<String>> {
        let host = region_host(url)?;
        // Non-cloud (direct / self-hosted) hosts have no regions.
        if !is_cloud_host(&host) {
            return Ok(vec![]);
        }

        let cache = region_cache();

        // Fast path: a fresh entry needs neither a fetch nor the fetch lock.
        let stale = match cache.get(&host) {
            Cached::Fresh(urls) => return Ok(urls),
            Cached::Stale(urls) => Some(urls),
            Cached::Miss => None,
        };

        // Single-flight: serialise concurrent fetches for the same host so they
        // collapse into one network request.
        let host_lock = fetch_lock(&host);
        let _guard = host_lock.lock().await;

        // Another caller may have refreshed the entry while we waited on the lock.
        if let Cached::Fresh(urls) = cache.get(&host) {
            return Ok(urls);
        }

        let endpoint = region_endpoint(url)?;
        match fetch_from_endpoint(&endpoint, token).await {
            Ok((urls, max_age)) => {
                cache.insert(host, urls.clone(), max_age);
                Ok(urls)
            }
            // The fresh fetch failed; fall back to the stale entry if we have
            // one rather than failing outright.
            Err(err) => match stale {
                Some(urls) => {
                    log::warn!(
                        "region fetch failed ({err}); using stale cached regions for {host}"
                    );
                    Ok(urls)
                }
                None => Err(err),
            },
        }
    }

    /// Reports that `failed_url` (a region URL previously returned for `url`'s
    /// host) could not be connected to, dropping it from the cache so it is not
    /// handed out again. When the host's last region URL is dropped the whole
    /// entry is invalidated, forcing a fresh fetch on the next attempt.
    pub fn mark_failed(url: &str, failed_url: &str) {
        if let Ok(host) = region_host(url) {
            region_cache().mark_failed(&host, failed_url);
        }
    }

    /// Invalidates the cached region list for `url`'s host, forcing a fresh
    /// fetch on the next [`Self::fetch_region_urls`] call.
    pub fn invalidate(url: &str) {
        if let Ok(host) = region_host(url) {
            region_cache().invalidate(&host);
        }
    }

    /// Clears the entire region cache. Useful when external state that affects
    /// geo routing changes (e.g. the device's network connectivity), since that
    /// can invalidate every cached host at once.
    #[allow(dead_code)]
    pub fn clear() {
        region_cache().clear();
    }
}

/// Fetches the region list from `endpoint_url`, returning the ordered URLs
/// together with the server's `Cache-Control: max-age` (if any) so the caller
/// can use it as the cache TTL.
pub(crate) async fn fetch_from_endpoint(
    endpoint_url: &str,
    token: &str,
) -> SignalResult<(Vec<String>, Option<Duration>)> {
    let transport = livekit_net::transport()
        .ok_or_else(|| SignalError::RegionError("no platform transport registered".into()))?;
    let headers = super::bearer_headers(token);
    let endpoint_url = endpoint_url.to_string();

    let fetch_fut = async {
        let res = transport
            .http_get(endpoint_url, headers)
            .await
            .map_err(|e| SignalError::RegionError(error_with_chain(&e)))?;
        let status =
            http::StatusCode::from_u16(res.status).unwrap_or(http::StatusCode::BAD_GATEWAY);
        if !status.is_success() {
            return Err(SignalError::Client(
                status,
                String::from_utf8_lossy(&res.body).into_owned(),
            ));
        }

        // Cache lifetime from the server's `Cache-Control: max-age`, if present.
        let max_age = res
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("cache-control"))
            .and_then(|h| parse_max_age(&h.value));

        let parsed: RegionsResponse = serde_json::from_slice(&res.body)
            .map_err(|e| SignalError::RegionError(error_with_chain(&e)))?;
        Ok((parsed.regions.into_iter().map(|i| i.url).collect(), max_age))
    };

    livekit_runtime::timeout(REGION_FETCH_TIMEOUT, fetch_fut)
        .await
        .map_err(|_| SignalError::RegionError("region fetch timed out".into()))?
}

fn region_endpoint(url: &str) -> SignalResult<String> {
    let mut url = url::Url::parse(url).map_err(|err| SignalError::UrlParse(err.to_string()))?;
    match url.scheme() {
        "wss" => url.set_scheme("https").unwrap(),
        "ws" => url.set_scheme("http").unwrap(),
        _ => (),
    }
    url.set_path("/settings/regions");

    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt;
    use std::io;

    // Mock error types to test error chain preservation
    #[derive(Debug)]
    struct RootCauseError {
        message: String,
    }

    impl fmt::Display for RootCauseError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for RootCauseError {}

    #[derive(Debug)]
    struct MiddleError {
        message: String,
        source: RootCauseError,
    }

    impl fmt::Display for MiddleError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for MiddleError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.source)
        }
    }

    #[derive(Debug)]
    struct OuterError {
        message: String,
        source: MiddleError,
    }

    impl fmt::Display for OuterError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for OuterError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.source)
        }
    }

    #[test]
    fn test_error_with_chain_single_error() {
        let err = RootCauseError { message: "root cause".to_string() };
        let result = error_with_chain(&err);
        assert_eq!(result, "root cause");
    }

    #[test]
    fn test_error_with_chain_two_level_chain() {
        let root =
            RootCauseError { message: "invalid peer certificate: UnknownIssuer".to_string() };
        let middle = MiddleError { message: "error trying to connect".to_string(), source: root };
        let result = error_with_chain(&middle);
        assert_eq!(result, "error trying to connect: invalid peer certificate: UnknownIssuer");
    }

    #[test]
    fn test_error_with_chain_three_level_chain() {
        // Simulates the actual error chain from reqwest -> hyper -> TLS
        let root =
            RootCauseError { message: "invalid peer certificate: UnknownIssuer".to_string() };
        let middle = MiddleError { message: "error trying to connect".to_string(), source: root };
        let outer = OuterError {
            message:
                "error sending request for url (https://example.livekit.cloud/settings/regions)"
                    .to_string(),
            source: middle,
        };
        let result = error_with_chain(&outer);
        assert_eq!(
            result,
            "error sending request for url (https://example.livekit.cloud/settings/regions): error trying to connect: invalid peer certificate: UnknownIssuer"
        );
    }

    #[test]
    fn test_error_with_chain_preserves_tls_error_info() {
        // Verify that TLS-specific error messages are preserved in the chain
        let root =
            RootCauseError { message: "invalid peer certificate: UnknownIssuer".to_string() };
        let outer = MiddleError { message: "TLS connection error".to_string(), source: root };
        let result = error_with_chain(&outer);

        // The error message should contain both the outer message and the root cause
        assert!(result.contains("TLS connection error"));
        assert!(result.contains("UnknownIssuer"));
        assert!(result.contains("invalid peer certificate"));
    }

    #[test]
    fn test_region_error_includes_full_chain() {
        // Test that SignalError::RegionError properly includes the full error chain
        let root =
            RootCauseError { message: "invalid peer certificate: UnknownIssuer".to_string() };
        let middle = MiddleError { message: "error trying to connect".to_string(), source: root };
        let outer = OuterError { message: "error sending request".to_string(), source: middle };

        let signal_error = SignalError::RegionError(error_with_chain(&outer));
        let error_string = signal_error.to_string();

        // Verify the full chain is in the error message
        assert!(
            error_string.contains("UnknownIssuer"),
            "Error should contain root cause 'UnknownIssuer', got: {}",
            error_string
        );
        assert!(
            error_string.contains("error trying to connect"),
            "Error should contain middle error, got: {}",
            error_string
        );
        assert!(
            error_string.contains("error sending request"),
            "Error should contain outer error, got: {}",
            error_string
        );
    }

    #[test]
    fn test_error_with_chain_io_error() {
        // Test with a real std::io::Error chain
        let inner = io::Error::new(io::ErrorKind::ConnectionRefused, "connection refused");
        let outer = io::Error::new(io::ErrorKind::Other, inner);

        let result = error_with_chain(&outer);
        assert!(
            result.contains("connection refused"),
            "Should contain the inner error message, got: {}",
            result
        );
    }

    #[test]
    fn test_region_host() {
        assert_eq!(region_host("wss://myapp.livekit.cloud").unwrap(), "myapp.livekit.cloud");
        assert_eq!(region_host("https://myapp.livekit.cloud/rtc").unwrap(), "myapp.livekit.cloud");
        assert!(region_host("not a url").is_err());
    }

    #[test]
    fn fetch_lock_is_shared_per_host() {
        // Same host hands back the same lock, so concurrent callers contend on a
        // single fetch; distinct hosts get independent locks. (RegionCache's own
        // caching behavior is unit-tested in crate::region.)
        let a1 = fetch_lock("a.livekit.cloud");
        let a2 = fetch_lock("a.livekit.cloud");
        let b = fetch_lock("b.livekit.cloud");
        assert!(Arc::ptr_eq(&a1, &a2), "same host shares one fetch lock");
        assert!(!Arc::ptr_eq(&a1, &b), "different hosts get distinct fetch locks");
    }

    #[test]
    fn test_region_endpoint() {
        assert_eq!(
            region_endpoint("wss://myapp.livekit.cloud").unwrap(),
            "https://myapp.livekit.cloud/settings/regions"
        );
        assert_eq!(
            region_endpoint("ws://myapp.livekit.run").unwrap(),
            "http://myapp.livekit.run/settings/regions"
        );
        assert_eq!(
            region_endpoint("https://myapp.livekit.cloud").unwrap(),
            "https://myapp.livekit.cloud/settings/regions"
        );
    }

    #[tokio::test]
    async fn test_fetch_non_cloud_url_returns_empty() {
        let result =
            RegionUrlProvider::fetch_region_urls("wss://localhost:7880", "fake-token").await;
        assert_eq!(result.unwrap(), Vec::<String>::new());
    }
}
