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

use std::{sync::OnceLock, time::Duration};

use http::header::{HeaderMap, CONTENT_LENGTH, CONTENT_TYPE};
use url::Url;

use crate::region::{is_cloud_host, parse_max_age, Cached, RegionCache, RegionsResponse};

/// Total attempts (the original request plus fallback regions) and the base
/// retry backoff are fixed, not user-configurable, so retries can't be tuned to
/// values that could overwhelm the server.
pub(crate) const MAX_ATTEMPTS: u32 = 3;
pub(crate) const BACKOFF_BASE: Duration = Duration::from_millis(200);

/// Default per-request timeout, applied to each attempt. Calls that dial a
/// phone (see [`crate::services::sip`]) override it with a longer budget.
pub(crate) const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Below this per-request timeout a retry is unlikely to help, and many clients
/// would retry in lockstep across regions, so a short request gets a single
/// attempt (thundering-herd guard).
pub(crate) const MIN_FAILOVER_TIMEOUT: Duration = Duration::from_secs(5);

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
    /// engages when enabled, the host is a LiveKit Cloud domain, and the
    /// per-attempt `timeout` is long enough that retrying is worthwhile. `force`
    /// bypasses the cloud-host check and is for internal testing only.
    pub(crate) fn attempts(&self, host: Option<&str>, timeout: Duration) -> u32 {
        if !(self.enabled && (self.force || host.map(is_cloud_host).unwrap_or(false))) {
            return 1;
        }
        if timeout < MIN_FAILOVER_TIMEOUT {
            return 1;
        }
        MAX_ATTEMPTS
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

/// Sleeps for `d` before a retry, so failover backs off between attempts on
/// either backend (retrying with no delay would risk hammering the server).
/// `livekit_runtime::sleep` keeps the async path runtime-agnostic (it maps to
/// `tokio::time::sleep` or the async-std timer depending on the enabled feature).
pub(crate) async fn backoff_sleep(d: Duration) {
    if d.is_zero() {
        return;
    }
    livekit_runtime::sleep(d).await;
}

/// Region discovery (`/settings/regions`) uses a short timeout so a slow or
/// unreachable endpoint doesn't stall the failover path.
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(2);

/// Process-wide region cache for the API failover path. Owns the API instance of
/// the shared [`RegionCache`] (which stores `http(s)` URLs; see [`crate::region`]).
fn region_cache() -> &'static RegionCache {
    static CACHE: OnceLock<RegionCache> = OnceLock::new();
    CACHE.get_or_init(|| RegionCache::new(RegionCache::DEFAULT_TTL))
}

/// Returns the alternative region URLs for `base`, fetching `/settings/regions`
/// if the cache is stale. Best-effort: on a fetch failure it serves a stale
/// cached list when available, otherwise an empty list (the caller then stops
/// failing over). Forwards the caller's headers so a valid token — and any test
/// directives — reach the discovery endpoint.
pub(crate) async fn region_urls(base: &Url, headers: &HeaderMap) -> Vec<String> {
    let key = host_key(base);
    let cache = region_cache();

    let stale = match cache.get(&key) {
        Cached::Fresh(urls) => return urls,
        Cached::Stale(urls) => Some(urls),
        Cached::Miss => None,
    };

    match fetch(base, headers).await {
        Ok((urls, max_age)) => {
            // A zero max-age (e.g. `Cache-Control: max-age=0`) means "do not
            // cache"; skip the insert so the next call re-fetches rather than
            // serving a stale entry. An absent max-age uses the cache default.
            if max_age != Some(Duration::ZERO) {
                cache.insert(key, urls.clone(), max_age);
            }
            urls
        }
        // The fresh fetch failed; fall back to the stale entry if we have one.
        Err(()) => stale.unwrap_or_default(),
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

/// Reads the `Cache-Control: max-age` from a header value for use as the cache
/// TTL; `None` (absent or unparseable) falls back to the cache's default TTL.
fn max_age_from_cache_control(value: Option<&str>) -> Option<Duration> {
    value.and_then(parse_max_age)
}

#[cfg(feature = "services-tokio")]
async fn fetch(base: &Url, headers: &HeaderMap) -> Result<(Vec<String>, Option<Duration>), ()> {
    use crate::url_with_path_suffix;

    let url = url_with_path_suffix(base, "/settings/regions");

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
    let max_age = max_age_from_cache_control(
        resp.headers().get("cache-control").and_then(|v| v.to_str().ok()),
    );
    let list: RegionsResponse = resp.json().await.map_err(|_| ())?;
    Ok((normalize(list), max_age))
}

#[cfg(all(feature = "services-async", not(feature = "services-tokio")))]
async fn fetch(base: &Url, headers: &HeaderMap) -> Result<(Vec<String>, Option<Duration>), ()> {
    use crate::url_with_path_suffix;
    use isahc::config::Configurable;
    use isahc::AsyncReadResponseExt;

    let url = url_with_path_suffix(base, "/settings/regions");

    let mut builder = isahc::Request::get(url.as_str()).timeout(DISCOVERY_TIMEOUT);
    // isahc vendors `http` 0.2, so pass name/value as &str/&[u8] to stay agnostic
    // to the `http` version the workspace `HeaderMap` (1.x) is built from.
    for (name, value) in forward_headers(headers).iter() {
        builder = builder.header(name.as_str(), value.as_bytes());
    }
    let request = builder.body(()).map_err(|_| ())?;
    let mut resp = isahc::send_async(request).await.map_err(|_| ())?;
    if !resp.status().is_success() {
        return Err(());
    }
    let max_age = max_age_from_cache_control(
        resp.headers().get("cache-control").and_then(|v| v.to_str().ok()),
    );
    let list: RegionsResponse = resp.json().await.map_err(|_| ())?;
    Ok((normalize(list), max_age))
}
