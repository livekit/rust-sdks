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

//! Region-discovery primitives shared by the two `/settings/regions` consumers:
//! the signaling region provider ([`crate::signal_client::region`]) and the API
//! failover region cache ([`crate::services::failover`]).
//!
//! Only the feature-independent pieces live here. The caches themselves are
//! deliberately separate: the signaling path keeps `wss://` URLs, de-duplicates
//! in-flight fetches and prunes failed regions, while the API path rewrites URLs
//! to `http(s)` and forwards the caller's request headers. The two are also in
//! independently compiled feature islands (the API SDK builds without the signal
//! client), so neither can depend on the other.

use std::time::Duration;

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
}
