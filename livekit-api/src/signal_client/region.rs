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

use http::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use parking_lot::Mutex;
use serde::Deserialize;

use crate::http_client;

use super::{SignalError, SignalResult, REGION_FETCH_TIMEOUT};

struct CachedRegions {
    urls: Vec<String>,
    fetched_at: Instant,
}

/// Process-wide region-list cache keyed by host, mirroring client-sdk-js's
/// static `RegionUrlProvider.cache`. Persisting it here (rather than on a
/// per-connection object) means it survives across reconnect attempts — each of
/// which rebuilds the SignalClient — so the reconnect loop does not re-pay the
/// region fetch on every attempt.
struct RegionCache {
    entries: Mutex<HashMap<String, CachedRegions>>,
    ttl: Duration,
}

impl RegionCache {
    /// How long a fetched region list is reused before being re-fetched. Matches
    /// client-sdk-js's `DEFAULT_MAX_AGE_MS`. (The server's `Cache-Control: max-age`
    /// is not yet honoured — a fixed TTL keeps this backend-agnostic; honouring
    /// the header is a possible refinement.)
    const TTL: Duration = Duration::from_secs(5);

    fn shared() -> &'static RegionCache {
        static CACHE: OnceLock<RegionCache> = OnceLock::new();
        CACHE.get_or_init(|| Self::new(Self::TTL))
    }

    fn new(ttl: Duration) -> Self {
        Self { entries: Mutex::new(HashMap::new()), ttl }
    }

    /// Cached region URLs for `host` if the entry is still within [`Self::ttl`],
    /// else `None` (the caller should re-fetch).
    fn get(&self, host: &str) -> Option<Vec<String>> {
        let entries = self.entries.lock();
        entries.get(host).filter(|e| e.fetched_at.elapsed() < self.ttl).map(|e| e.urls.clone())
    }

    fn insert(&self, host: String, urls: Vec<String>) {
        self.entries.lock().insert(host, CachedRegions { urls, fetched_at: Instant::now() });
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
    /// returns an empty list. Successful results are cached per host for
    /// [`RegionCache::TTL`]; failures are never cached.
    pub async fn fetch_region_urls(url: &str, token: &str) -> SignalResult<Vec<String>> {
        if !is_cloud_url(url)? {
            return Ok(vec![]);
        }

        let host = region_host(url)?;
        if let Some(urls) = RegionCache::shared().get(&host) {
            return Ok(urls);
        }

        let endpoint = region_endpoint(url)?;
        let urls = fetch_from_endpoint(&endpoint, token).await?;
        RegionCache::shared().insert(host, urls.clone());
        Ok(urls)
    }
}

pub(crate) async fn fetch_from_endpoint(
    endpoint_url: &str,
    token: &str,
) -> SignalResult<Vec<String>> {
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
        let res = res
            .json::<RegionUrlResponse>()
            .await
            .map_err(|e| SignalError::RegionError(error_with_chain(&e)))?;
        Ok(res.regions.into_iter().map(|i| i.url).collect())
    };

    livekit_runtime::timeout(REGION_FETCH_TIMEOUT, fetch_fut)
        .await
        .map_err(|_| SignalError::RegionError("region fetch timed out".into()))?
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
    fn region_cache_hits_fresh_and_misses_unknown_or_stale() {
        let cache = RegionCache::new(RegionCache::TTL);

        let host = "cache-fresh.livekit.cloud";
        assert!(cache.get(host).is_none(), "unknown host is a miss");

        let urls = vec!["wss://r1.livekit.cloud".to_string(), "wss://r2.livekit.cloud".to_string()];
        cache.insert(host.to_string(), urls.clone());
        assert_eq!(cache.get(host), Some(urls), "fresh entry is a hit");

        // A stale entry (fetched older than the TTL) is treated as a miss.
        let stale_host = "cache-stale.livekit.cloud";
        if let Some(past) = Instant::now().checked_sub(RegionCache::TTL * 2) {
            cache.entries.lock().insert(
                stale_host.to_string(),
                CachedRegions { urls: vec!["wss://old.livekit.cloud".into()], fetched_at: past },
            );
            assert!(cache.get(stale_host).is_none(), "stale entry is a miss");
        }
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
