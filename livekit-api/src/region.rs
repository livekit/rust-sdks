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

//! Region discovery shared by the two `/settings/regions` consumers: the
//! signaling region provider ([`crate::signal_client::region_url_provider`]) and
//! the API failover ([`crate::services::failover`]).
//!
//! This holds the runtime-independent pieces both need: the response types, the
//! cloud-host / `Cache-Control` helpers, and the [`RegionCache`] (TTL lookups,
//! stale fallback, pruning). Each consumer owns its own cache instance — they
//! store URLs in different schemes (`wss://` for signaling, `http(s)` for the API)
//! — but the caching logic lives here once. The network fetch stays with each
//! consumer: they authenticate differently (bearer token vs. forwarded headers)
//! and run on different HTTP clients / feature islands.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Deserialize;

/// Response body of the LiveKit Cloud `/settings/regions` endpoint. Extra fields
/// (`region`, `distance`, …) are ignored; both consumers only need the URLs. A
/// body missing `regions` is rejected so a malformed discovery response surfaces
/// as an error rather than an empty region list.
#[derive(Debug, Deserialize)]
pub(crate) struct RegionsResponse {
    pub regions: Vec<RegionInfo>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RegionInfo {
    pub url: String,
}

/// Reports whether `host` belongs to a LiveKit Cloud project — a
/// `*.livekit.cloud` or `*.livekit.run` subdomain. Region discovery and API
/// failover only engage for these hosts.
pub(crate) fn is_cloud_host(host: &str) -> bool {
    host.ends_with(".livekit.cloud") || host.ends_with(".livekit.run")
}

/// Parses the `max-age` directive (in seconds) out of a `Cache-Control` header
/// value, e.g. `"public, max-age=300"` -> `Some(300s)`. Returns `None` when the
/// directive is absent or unparseable. Directive names are case-insensitive.
pub(crate) fn parse_max_age(cache_control: &str) -> Option<Duration> {
    cache_control.split(',').find_map(|directive| {
        let (name, value) = directive.split_once('=')?;
        name.trim().eq_ignore_ascii_case("max-age").then_some(())?;
        value.trim().parse::<u64>().ok().map(Duration::from_secs)
    })
}

struct CachedRegions {
    urls: Vec<String>,
    fetched_at: Instant,
    /// Effective lifetime of this entry: the server's `Cache-Control: max-age`
    /// when present, otherwise [`RegionCache::DEFAULT_TTL`].
    ttl: Duration,
}

/// Outcome of a [`RegionCache::get`] lookup.
pub(crate) enum Cached {
    /// Entry exists and is within the TTL — safe to use without re-fetching.
    Fresh(Vec<String>),
    /// Entry exists but is older than the TTL — the caller should re-fetch, but
    /// may fall back to these URLs if the re-fetch fails.
    Stale(Vec<String>),
    /// No entry for this host.
    Miss,
}

/// Process-wide region-list cache keyed by host. Each consumer owns its own
/// instance (see the module docs); this type holds the shared caching logic:
/// TTL derived from the server's `Cache-Control: max-age`, fresh/stale/miss
/// lookups (stale entries are retained for fallback), and pruning of failed
/// regions.
pub(crate) struct RegionCache {
    entries: Mutex<HashMap<String, CachedRegions>>,
    default_ttl: Duration,
}

impl RegionCache {
    /// Fallback entry lifetime, used when the region response carries no
    /// `Cache-Control: max-age`. Matches client-sdk-js's `DEFAULT_MAX_AGE_MS`.
    pub(crate) const DEFAULT_TTL: Duration = Duration::from_secs(5);

    pub(crate) fn new(default_ttl: Duration) -> Self {
        Self { entries: Mutex::new(HashMap::new()), default_ttl }
    }

    /// Looks up the cached region URLs for `host`, reporting whether the entry is
    /// fresh (within its TTL), stale, or absent.
    pub(crate) fn get(&self, host: &str) -> Cached {
        let entries = self.entries.lock().unwrap();
        match entries.get(host) {
            Some(e) if e.fetched_at.elapsed() < e.ttl => Cached::Fresh(e.urls.clone()),
            Some(e) => Cached::Stale(e.urls.clone()),
            None => Cached::Miss,
        }
    }

    /// Stores `urls` for `host`, honouring the server's `Cache-Control: max-age`
    /// (`max_age`) as the entry's TTL and falling back to [`Self::DEFAULT_TTL`]
    /// when the header is absent.
    pub(crate) fn insert(&self, host: String, urls: Vec<String>, max_age: Option<Duration>) {
        let ttl = max_age.unwrap_or(self.default_ttl);
        self.entries
            .lock()
            .unwrap()
            .insert(host, CachedRegions { urls, fetched_at: Instant::now(), ttl });
    }

    /// Removes `failed_url` from the cached list for `host` so it is not handed
    /// out again. If that empties the list, the entry is dropped entirely,
    /// forcing a re-fetch on the next lookup.
    // Only the signaling consumer prunes individual failed regions.
    #[allow(dead_code)]
    pub(crate) fn mark_failed(&self, host: &str, failed_url: &str) {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get_mut(host) {
            entry.urls.retain(|u| u != failed_url);
            if entry.urls.is_empty() {
                entries.remove(host);
            }
        }
    }

    /// Drops the cached entry for `host`, forcing a re-fetch on the next lookup.
    #[allow(dead_code)]
    pub(crate) fn invalidate(&self, host: &str) {
        self.entries.lock().unwrap().remove(host);
    }

    /// Drops every cached entry.
    #[allow(dead_code)]
    pub(crate) fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_cloud_host() {
        assert!(is_cloud_host("myapp.livekit.cloud"));
        assert!(is_cloud_host("myapp.region.livekit.cloud"));
        assert!(is_cloud_host("myapp.livekit.run"));
        assert!(!is_cloud_host("localhost"));
        assert!(!is_cloud_host("example.com"));
        assert!(!is_cloud_host("livekit.cloud.example.com"));
        assert!(!is_cloud_host("127.0.0.1"));
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
            cache.entries.lock().unwrap().insert(
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
    fn region_cache_mark_failed_prunes_then_drops() {
        let cache = RegionCache::new(RegionCache::DEFAULT_TTL);
        let host = "mark-failed.livekit.cloud";
        let r1 = "wss://r1.livekit.cloud".to_string();
        let r2 = "wss://r2.livekit.cloud".to_string();
        cache.insert(host.to_string(), vec![r1.clone(), r2.clone()], None);

        // Pruning one URL keeps the entry with the survivors.
        cache.mark_failed(host, &r1);
        assert!(
            matches!(cache.get(host), Cached::Fresh(u) if u == vec![r2.clone()]),
            "failed URL is pruned, the rest remain"
        );

        // Removing the last URL drops the entry entirely, forcing a re-fetch.
        cache.mark_failed(host, &r2);
        assert!(matches!(cache.get(host), Cached::Miss), "emptied entry is dropped");

        // Marking an unknown host is a no-op.
        cache.mark_failed("unknown.livekit.cloud", &r1);
    }

    #[test]
    fn region_cache_invalidate_and_clear() {
        let cache = RegionCache::new(RegionCache::DEFAULT_TTL);
        let a = "a.livekit.cloud";
        let b = "b.livekit.cloud";
        let urls = vec!["wss://r.livekit.cloud".to_string()];
        cache.insert(a.to_string(), urls.clone(), None);
        cache.insert(b.to_string(), urls.clone(), None);

        // invalidate drops only the targeted host.
        cache.invalidate(a);
        assert!(matches!(cache.get(a), Cached::Miss), "invalidated host is a miss");
        assert!(matches!(cache.get(b), Cached::Fresh(_)), "other host is untouched");

        // clear drops everything.
        cache.clear();
        assert!(matches!(cache.get(b), Cached::Miss), "clear removes all entries");
    }
}
