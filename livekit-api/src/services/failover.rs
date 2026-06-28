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

//! Region failover for the Twirp API clients.
//!
//! On a retryable failure (any transport error or HTTP 5xx) the [`TwirpClient`]
//! discovers alternative LiveKit Cloud regions via `/settings/regions` and
//! replays the request against the next region, with exponential backoff. 4xx
//! responses are returned immediately. See [`TwirpClient::request`].

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use http::header::{HeaderMap, CONTENT_LENGTH, CONTENT_TYPE};
use url::Url;

use crate::region::{is_cloud_host, parse_max_age, RegionsResponse};

/// Total attempts (the original request plus fallback regions) and the base
/// retry backoff are fixed, not user-configurable, so retries can't be tuned to
/// values that could overwhelm the server.
pub(crate) const MAX_ATTEMPTS: u32 = 3;
pub(crate) const BACKOFF_BASE: Duration = Duration::from_millis(200);

/// Internal region-failover configuration. The public API exposes only the
/// `enabled` toggle (default true); `force` and `backoff_base` are test-only.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FailoverConfig {
    pub enabled: bool,
    /// Bypasses the cloud-host check. Internal testing only.
    pub force: bool,
    /// Retry backoff base. Internal testing only.
    pub backoff_base: Duration,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self { enabled: true, force: false, backoff_base: BACKOFF_BASE }
    }
}

impl FailoverConfig {
    /// Total request attempts for a host; 1 means no failover. Failover only
    /// engages when enabled and the host is a LiveKit Cloud domain. `force`
    /// bypasses the cloud-host check and is for internal testing only.
    pub(crate) fn attempts(&self, host: Option<&str>) -> u32 {
        if self.enabled && (self.force || host.map(is_cloud_host).unwrap_or(false)) {
            MAX_ATTEMPTS
        } else {
            1
        }
    }
}

/// Normalizes a region URL to an http(s) scheme (ws -> http, wss -> https),
/// mirroring the other SDKs and the server.
fn to_http_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("ws") {
        format!("http{rest}")
    } else {
        url.to_owned()
    }
}

/// A stable key identifying a host (including port) for dedup across attempts.
pub(crate) fn host_key(url: &Url) -> String {
    format!("{}:{}", url.host_str().unwrap_or(""), url.port_or_known_default().unwrap_or(0))
}

/// Returns the first region URL whose host has not yet been attempted.
pub(crate) fn pick_next(region_urls: &[String], attempted: &[String]) -> Option<Url> {
    for raw in region_urls {
        let Ok(url) = Url::parse(raw) else { continue };
        if url.host_str().is_none() {
            continue;
        }
        if !attempted.iter().any(|a| a == &host_key(&url)) {
            return Some(url);
        }
    }
    None
}

/// Sleeps for `d` before a retry. Backoff is applied on the tokio backend; the
/// async backend retries without delay (the failover path is short).
pub(crate) async fn backoff_sleep(d: Duration) {
    if d.is_zero() {
        return;
    }
    #[cfg(feature = "services-tokio")]
    {
        tokio::time::sleep(d).await;
    }
    #[cfg(not(feature = "services-tokio"))]
    {
        let _ = d;
    }
}

/// Region discovery (`/settings/regions`) uses a short timeout so a slow or
/// unreachable endpoint doesn't stall the failover path.
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(2);

struct CacheEntry {
    urls: Vec<String>,
    fetched_at: Instant,
    ttl: Duration,
}

fn cache() -> &'static Mutex<HashMap<String, CacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns the alternative region URLs for `base`, fetching `/settings/regions`
/// if the cache is stale. Best-effort: on a fetch failure it serves a stale
/// cached list when available, otherwise an empty list (the caller then stops
/// failing over). Forwards the caller's headers so a valid token — and any test
/// directives — reach the discovery endpoint.
pub(crate) async fn region_urls(base: &Url, headers: &HeaderMap) -> Vec<String> {
    let key = host_key(base);

    {
        let c = cache().lock().unwrap();
        if let Some(entry) = c.get(&key) {
            if entry.fetched_at.elapsed() < entry.ttl {
                return entry.urls.clone();
            }
        }
    }

    match fetch(base, headers).await {
        Ok((urls, ttl)) => {
            // A zero TTL (e.g. Cache-Control: max-age=0) means "do not cache".
            if !ttl.is_zero() {
                cache().lock().unwrap().insert(
                    key,
                    CacheEntry { urls: urls.clone(), fetched_at: Instant::now(), ttl },
                );
            }
            urls
        }
        Err(()) => cache().lock().unwrap().get(&key).map(|e| e.urls.clone()).unwrap_or_default(),
    }
}

/// Builds the header set forwarded to the discovery endpoint: the caller's
/// headers minus body-specific ones.
fn forward_headers(headers: &HeaderMap) -> HeaderMap {
    let mut out = headers.clone();
    out.remove(CONTENT_TYPE);
    out.remove(CONTENT_LENGTH);
    out
}

/// The discovery response carries `wss://` region URLs; the API client speaks
/// HTTP, so rewrite each to its `http(s)` equivalent.
fn normalize(list: RegionsResponse) -> Vec<String> {
    list.regions.into_iter().filter(|r| !r.url.is_empty()).map(|r| to_http_url(&r.url)).collect()
}

/// Reads the cache TTL from a `Cache-Control` header value; absent or
/// unparseable means a zero TTL ("do not cache").
fn ttl_from_cache_control(value: Option<&str>) -> Duration {
    value.and_then(parse_max_age).unwrap_or(Duration::ZERO)
}

#[cfg(feature = "services-tokio")]
async fn fetch(base: &Url, headers: &HeaderMap) -> Result<(Vec<String>, Duration), ()> {
    let mut url = base.clone();
    url.set_path("/settings/regions");

    let resp = reqwest::Client::new()
        .get(url)
        .headers(forward_headers(headers))
        .timeout(DISCOVERY_TIMEOUT)
        .send()
        .await
        .map_err(|_| ())?;
    if !resp.status().is_success() {
        return Err(());
    }
    let ttl =
        ttl_from_cache_control(resp.headers().get("cache-control").and_then(|v| v.to_str().ok()));
    let list: RegionsResponse = resp.json().await.map_err(|_| ())?;
    Ok((normalize(list), ttl))
}

#[cfg(all(feature = "services-async", not(feature = "services-tokio")))]
async fn fetch(base: &Url, headers: &HeaderMap) -> Result<(Vec<String>, Duration), ()> {
    use isahc::config::Configurable;
    use isahc::AsyncReadResponseExt;

    let mut url = base.clone();
    url.set_path("/settings/regions");

    let mut builder = isahc::Request::get(url.as_str()).timeout(DISCOVERY_TIMEOUT);
    for (name, value) in forward_headers(headers).iter() {
        builder = builder.header(name, value);
    }
    let request = builder.body(()).map_err(|_| ())?;
    let mut resp = isahc::send_async(request).await.map_err(|_| ())?;
    if !resp.status().is_success() {
        return Err(());
    }
    let ttl =
        ttl_from_cache_control(resp.headers().get("cache-control").and_then(|v| v.to_str().ok()));
    let list: RegionsResponse = resp.json().await.map_err(|_| ())?;
    Ok((normalize(list), ttl))
}
