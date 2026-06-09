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
    sync::OnceLock,
    time::{Duration, Instant},
};

use http::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use parking_lot::Mutex;
use serde::Deserialize;

use crate::http_client;

use super::{SignalError, SignalResult, REGION_FETCH_TIMEOUT};

/// How long a fetched region list is reused before being re-fetched. Matches
/// client-sdk-js's `DEFAULT_MAX_AGE_MS`. (The server's `Cache-Control: max-age`
/// is not yet honoured — a fixed TTL keeps this backend-agnostic; honouring the
/// header is a possible refinement.)
const REGION_CACHE_TTL: Duration = Duration::from_secs(5);

struct CachedRegions {
    urls: Vec<String>,
    fetched_at: Instant,
}

/// Process-wide region-list cache keyed by host, mirroring client-sdk-js's
/// static `RegionUrlProvider.cache`. Persisting it here (rather than on a
/// per-connection object) means it survives across reconnect attempts — each of
/// which rebuilds the SignalClient — so the reconnect loop does not re-pay the
/// region fetch on every attempt.
fn region_cache() -> &'static Mutex<HashMap<String, CachedRegions>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedRegions>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Cached region URLs for `host` if the entry is still within
/// [`REGION_CACHE_TTL`], else `None` (the caller should re-fetch).
fn cached_region_urls(host: &str) -> Option<Vec<String>> {
    let cache = region_cache().lock();
    cache.get(host).filter(|e| e.fetched_at.elapsed() < REGION_CACHE_TTL).map(|e| e.urls.clone())
}

fn store_region_urls(host: String, urls: Vec<String>) {
    region_cache().lock().insert(host, CachedRegions { urls, fetched_at: Instant::now() });
}

fn region_host(url: &str) -> SignalResult<String> {
    let parsed = url::Url::parse(url).map_err(|err| SignalError::UrlParse(err.to_string()))?;
    parsed
        .host_str()
        .map(|h| h.to_string())
        .ok_or_else(|| SignalError::UrlParse("invalid hostname".into()))
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
    /// [`REGION_CACHE_TTL`]; failures are never cached.
    pub async fn fetch_region_urls(url: &str, token: &str) -> SignalResult<Vec<String>> {
        if !is_cloud_url(url)? {
            return Ok(vec![]);
        }

        let host = region_host(url)?;
        if let Some(urls) = cached_region_urls(&host) {
            return Ok(urls);
        }

        let endpoint = region_endpoint(url)?;
        let urls = fetch_from_endpoint(&endpoint, token).await?;
        store_region_urls(host, urls.clone());
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
            .map_err(|e| SignalError::RegionError(e.to_string()))?;

        if !res.status().is_success() {
            return Err(SignalError::Client(res.status(), res.text().await.unwrap_or_default()));
        }
        let res = res
            .json::<RegionUrlResponse>()
            .await
            .map_err(|e| SignalError::RegionError(e.to_string()))?;
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
        // Unique hosts so the process-wide cache doesn't collide with other tests.
        let host = "cache-fresh.livekit.cloud";
        assert!(cached_region_urls(host).is_none(), "unknown host is a miss");

        let urls = vec!["wss://r1.livekit.cloud".to_string(), "wss://r2.livekit.cloud".to_string()];
        store_region_urls(host.to_string(), urls.clone());
        assert_eq!(cached_region_urls(host), Some(urls), "fresh entry is a hit");

        // A stale entry (fetched older than the TTL) is treated as a miss.
        let stale_host = "cache-stale.livekit.cloud";
        if let Some(past) = Instant::now().checked_sub(REGION_CACHE_TTL * 2) {
            region_cache().lock().insert(
                stale_host.to_string(),
                CachedRegions { urls: vec!["wss://old.livekit.cloud".into()], fetched_at: past },
            );
            assert!(cached_region_urls(stale_host).is_none(), "stale entry is a miss");
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
