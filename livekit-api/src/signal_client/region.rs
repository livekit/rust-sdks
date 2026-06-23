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
    sync::OnceLock,
    time::{Duration, Instant},
};

use http::header::{HeaderMap, HeaderValue, AUTHORIZATION, CACHE_CONTROL};
use parking_lot::Mutex;
use serde::Deserialize;

use crate::http_client;

use super::{SignalError, SignalResult, REGION_FETCH_TIMEOUT};

struct CachedRegions {
    urls: Vec<String>,
    fetched_at: Instant,
    /// Effective lifetime of this entry: the server's `Cache-Control: max-age`
    /// when present, otherwise [`RegionCache::default_ttl`].
    ttl: Duration,
}

/// Outcome of a [`RegionCache::get`] lookup.
enum Cached {
    /// Entry exists and is within the TTL — safe to use without re-fetching.
    Fresh(Vec<String>),
    /// Entry exists but is older than the TTL — the caller should re-fetch, but
    /// may fall back to these URLs if the re-fetch fails.
    Stale(Vec<String>),
    /// No entry for this host.
    Miss,
}

/// Process-wide region-list cache keyed by host, mirroring client-sdk-js's
/// static `RegionUrlProvider.cache`. Persisting it here (rather than on a
/// per-connection object) means it survives across reconnect attempts — each of
/// which rebuilds the SignalClient — so the reconnect loop does not re-pay the
/// region fetch on every attempt.
struct RegionCache {
    entries: Mutex<HashMap<String, CachedRegions>>,
    default_ttl: Duration,
}

impl RegionCache {
    /// Fallback entry lifetime, used when the server's region response carries
    /// no `Cache-Control: max-age`. Matches client-sdk-js's `DEFAULT_MAX_AGE_MS`.
    const DEFAULT_TTL: Duration = Duration::from_secs(5);

    fn shared() -> &'static RegionCache {
        static CACHE: OnceLock<RegionCache> = OnceLock::new();
        CACHE.get_or_init(|| Self::new(Self::DEFAULT_TTL))
    }

    fn new(default_ttl: Duration) -> Self {
        Self { entries: Mutex::new(HashMap::new()), default_ttl }
    }

    /// Looks up the cached region URLs for `host`, reporting whether the entry
    /// is fresh (within its TTL), stale, or absent. A stale entry is retained so
    /// callers can fall back to it when a re-fetch fails.
    fn get(&self, host: &str) -> Cached {
        let entries = self.entries.lock();
        match entries.get(host) {
            Some(e) if e.fetched_at.elapsed() < e.ttl => Cached::Fresh(e.urls.clone()),
            Some(e) => Cached::Stale(e.urls.clone()),
            None => Cached::Miss,
        }
    }

    /// Stores `urls` for `host`, honouring the server's `Cache-Control: max-age`
    /// (`max_age`) as the entry's TTL and falling back to [`Self::default_ttl`]
    /// when the header is absent.
    fn insert(&self, host: String, urls: Vec<String>, max_age: Option<Duration>) {
        let ttl = max_age.unwrap_or(self.default_ttl);
        self.entries.lock().insert(host, CachedRegions { urls, fetched_at: Instant::now(), ttl });
    }
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

#[derive(Deserialize)]
pub struct RegionUrlResponse {
    pub regions: Vec<RegionUrlInfo>,
}

#[derive(Deserialize)]
pub struct RegionUrlInfo {
    pub region: String,
    pub url: String,
    pub distance: String,
}

impl RegionUrlProvider {
    /// Fetch the ordered list of region signalling URLs for a LiveKit Cloud
    /// host. Non-cloud (direct / self-hosted) URLs have no regions, so this
    /// returns an empty list. Successful results are cached per host for the
    /// server's `Cache-Control: max-age` (or [`RegionCache::DEFAULT_TTL`] when
    /// absent); failures are never cached. Once an entry goes stale a re-fetch
    /// is attempted, but if it fails the stale entry is returned as a fallback
    /// rather than surfacing the error.
    pub async fn fetch_region_urls(url: &str, token: &str) -> SignalResult<Vec<String>> {
        if !is_cloud_url(url)? {
            return Ok(vec![]);
        }

        let host = region_host(url)?;
        let cache = RegionCache::shared();
        let stale = match cache.get(&host) {
            Cached::Fresh(urls) => return Ok(urls),
            Cached::Stale(urls) => Some(urls),
            Cached::Miss => None,
        };

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
                    log::warn!("region fetch failed ({err}); using stale cached regions for {host}");
                    Ok(urls)
                }
                None => Err(err),
            },
        }
    }
}

/// Fetches the region list from `endpoint_url`, returning the ordered URLs
/// together with the server's `Cache-Control: max-age` (if any) so the caller
/// can use it as the cache TTL.
pub(crate) async fn fetch_from_endpoint(
    endpoint_url: &str,
    token: &str,
) -> SignalResult<(Vec<String>, Option<Duration>)> {
    let fetch_fut = async {
        let client = http_client::Client::new();
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token)).unwrap());
        let res = client
            .get(endpoint_url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| SignalError::RegionError(error_with_chain(&e)))?;

        if !res.status().is_success() {
            return Err(SignalError::Client(res.status(), res.text().await.unwrap_or_default()));
        }

        // Read the cache lifetime before `json()` consumes the response.
        let max_age =
            res.headers().get(CACHE_CONTROL).and_then(|v| v.to_str().ok()).and_then(parse_max_age);

        let res = res
            .json::<RegionUrlResponse>()
            .await
            .map_err(|e| SignalError::RegionError(error_with_chain(&e)))?;
        Ok((res.regions.into_iter().map(|i| i.url).collect(), max_age))
    };

    livekit_runtime::timeout(REGION_FETCH_TIMEOUT, fetch_fut)
        .await
        .map_err(|_| SignalError::RegionError("region fetch timed out".into()))?
}

/// Parses the `max-age` directive (in seconds) out of a `Cache-Control` header
/// value, e.g. `"max-age=300, public"` -> `Some(300s)`. Returns `None` when the
/// directive is absent or unparseable, leaving the caller on the default TTL.
fn parse_max_age(cache_control: &str) -> Option<Duration> {
    cache_control.split(',').find_map(|directive| {
        let (name, value) = directive.split_once('=')?;
        name.trim().eq_ignore_ascii_case("max-age").then_some(())?;
        value.trim().parse::<u64>().ok().map(Duration::from_secs)
    })
}

fn is_cloud_url(url: &str) -> SignalResult<bool> {
    let url = url::Url::parse(url).map_err(|err| SignalError::UrlParse(err.to_string()))?;
    let host = match url.host() {
        Some(host) => host.to_string(),
        None => {
            return Err(SignalError::UrlParse("invalid hostname".into()));
        }
    };

    Ok(host.ends_with(".livekit.cloud") || host.ends_with(".livekit.run"))
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
    fn test_is_cloud_url() {
        assert!(is_cloud_url("wss://myapp.livekit.cloud").unwrap());
        assert!(is_cloud_url("wss://myapp.livekit.run").unwrap());
        assert!(is_cloud_url("https://myapp.livekit.cloud").unwrap());

        assert!(!is_cloud_url("wss://localhost:7880").unwrap());
        assert!(!is_cloud_url("wss://example.com").unwrap());
        assert!(!is_cloud_url("wss://livekit.cloud.example.com").unwrap());
    }

    #[test]
    fn test_region_host() {
        assert_eq!(region_host("wss://myapp.livekit.cloud").unwrap(), "myapp.livekit.cloud");
        assert_eq!(region_host("https://myapp.livekit.cloud/rtc").unwrap(), "myapp.livekit.cloud");
        assert!(region_host("not a url").is_err());
    }

    #[test]
    fn region_cache_reports_fresh_stale_and_miss() {
        let cache = RegionCache::new(RegionCache::DEFAULT_TTL);

        let host = "cache-fresh.livekit.cloud";
        assert!(matches!(cache.get(host), Cached::Miss), "unknown host is a miss");

        let urls = vec!["wss://r1.livekit.cloud".to_string(), "wss://r2.livekit.cloud".to_string()];
        cache.insert(host.to_string(), urls.clone(), None);
        assert!(
            matches!(cache.get(host), Cached::Fresh(u) if u == urls),
            "fresh entry is a fresh hit"
        );

        // An entry older than its TTL is reported as stale (retained for fallback).
        let stale_host = "cache-stale.livekit.cloud";
        let stale_urls = vec!["wss://old.livekit.cloud".to_string()];
        if let Some(past) = Instant::now().checked_sub(RegionCache::DEFAULT_TTL * 2) {
            cache.entries.lock().insert(
                stale_host.to_string(),
                CachedRegions {
                    urls: stale_urls.clone(),
                    fetched_at: past,
                    ttl: RegionCache::DEFAULT_TTL,
                },
            );
            assert!(
                matches!(cache.get(stale_host), Cached::Stale(u) if u == stale_urls),
                "expired entry is a stale hit"
            );
        }
    }

    #[test]
    fn region_cache_honors_server_max_age() {
        // A short max-age expires before the (longer) default TTL would, proving
        // the server's Cache-Control wins over the default.
        let cache = RegionCache::new(Duration::from_secs(3600));
        let host = "max-age.livekit.cloud";
        let urls = vec!["wss://r1.livekit.cloud".to_string()];

        cache.insert(host.to_string(), urls.clone(), Some(Duration::ZERO));
        assert!(
            matches!(cache.get(host), Cached::Stale(u) if u == urls),
            "max-age=0 entry is immediately stale despite the long default TTL"
        );
    }

    #[test]
    fn test_parse_max_age() {
        assert_eq!(parse_max_age("max-age=300"), Some(Duration::from_secs(300)));
        assert_eq!(parse_max_age("public, max-age=300"), Some(Duration::from_secs(300)));
        assert_eq!(parse_max_age("MAX-AGE=0, no-cache"), Some(Duration::ZERO));
        assert_eq!(parse_max_age("no-store"), None);
        assert_eq!(parse_max_age("max-age=notanumber"), None);
        assert_eq!(parse_max_age(""), None);
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
