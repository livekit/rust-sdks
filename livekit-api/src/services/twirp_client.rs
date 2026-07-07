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

use std::{fmt::Display, time::Duration};

use http::{
    header::{HeaderMap, HeaderValue, CONTENT_TYPE, USER_AGENT},
    StatusCode,
};
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use super::failover::{self, FailoverConfig};
use crate::{http_client, url_with_path_suffix};

pub const DEFAULT_PREFIX: &str = "/twirp";

/// Identifies the SDK and version to the server on every request.
const USER_AGENT_VALUE: &str = concat!("livekit-server-sdk-rust/", env!("CARGO_PKG_VERSION"));

/// LiveKit URLs are commonly `wss://` (or `ws://`); the server APIs are Twirp
/// over HTTP, so the scheme is normalized to `https://` (or `http://`).
fn normalize_host(host: &str) -> String {
    if let Some(rest) = host.strip_prefix("wss://") {
        format!("https://{rest}")
    } else if let Some(rest) = host.strip_prefix("ws://") {
        format!("http://{rest}")
    } else {
        host.to_owned()
    }
}

#[cfg(test)]
mod normalize_host_tests {
    use super::normalize_host;

    #[test]
    fn normalizes_ws_schemes() {
        assert_eq!(normalize_host("wss://my.livekit.cloud"), "https://my.livekit.cloud");
        assert_eq!(normalize_host("ws://localhost:7880"), "http://localhost:7880");
        assert_eq!(normalize_host("https://my.livekit.cloud"), "https://my.livekit.cloud");
        assert_eq!(normalize_host("http://localhost:7880"), "http://localhost:7880");
    }
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[cfg(feature = "services-tokio")]
    #[error("failed to execute the request: {0}")]
    Request(#[from] reqwest::Error),
    #[cfg(feature = "services-async")]
    #[error("failed to execute the request: {0}")]
    Request(#[from] std::io::Error),
    #[error("server error: {0}")]
    Twirp(ServerErrorCode),
    #[error("url error: {0}")]
    Url(#[from] url::ParseError),
    #[error("prost error: {0}")]
    Prost(#[from] prost::DecodeError),
}

/// Deprecated alias for [`ServerError`], kept for backwards compatibility.
pub type TwirpError = ServerError;

#[derive(Debug, Deserialize)]
pub struct ServerErrorCode {
    pub code: String,
    pub msg: String,
    /// Extra key/value context the server attached (e.g. SIP status). Absent on
    /// most errors.
    #[serde(default)]
    pub meta: std::collections::HashMap<String, String>,
}

impl ServerErrorCode {
    pub const CANCELED: &'static str = "canceled";
    pub const UNKNOWN: &'static str = "unknown";
    pub const INVALID_ARGUMENT: &'static str = "invalid_argument";
    pub const MALFORMED: &'static str = "malformed";
    pub const DEADLINE_EXCEEDED: &'static str = "deadline_exceeded";
    pub const NOT_FOUND: &'static str = "not_found";
    pub const BAD_ROUTE: &'static str = "bad_route";
    pub const ALREADY_EXISTS: &'static str = "already_exists";
    pub const PERMISSION_DENIED: &'static str = "permission_denied";
    pub const UNAUTHENTICATED: &'static str = "unauthenticated";
    pub const RESOURCE_EXHAUSTED: &'static str = "resource_exhausted";
    pub const FAILED_PRECONDITION: &'static str = "failed_precondition";
    pub const ABORTED: &'static str = "aborted";
    pub const OUT_OF_RANGE: &'static str = "out_of_range";
    pub const UNIMPLEMENTED: &'static str = "unimplemented";
    pub const INTERNAL: &'static str = "internal";
    pub const UNAVAILABLE: &'static str = "unavailable";
    pub const DATA_LOSS: &'static str = "dataloss";
}

impl Display for ServerErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.msg)
    }
}

pub type ServerResult<T> = Result<T, ServerError>;

/// Deprecated alias for [`ServerErrorCode`], kept for backwards compatibility.
pub type TwirpErrorCode = ServerErrorCode;

/// Deprecated alias for [`ServerResult`], kept for backwards compatibility.
pub type TwirpResult<T> = ServerResult<T>;

#[derive(Debug)]
pub struct TwirpClient {
    host: String,
    pkg: String,
    prefix: String,
    client: http_client::Client,
    failover: FailoverConfig,
    request_timeout: Duration,
    // Headers added to every request; used by tests to inject mock directives
    // since the public service methods don't expose per-call headers.
    #[cfg(test)]
    default_headers: HeaderMap,
}

impl TwirpClient {
    pub fn new(host: &str, pkg: &str, prefix: Option<&str>) -> Self {
        Self::with_client(host, pkg, prefix, http_client::Client::new())
    }

    /// Like [`new`](Self::new) but reuses an existing HTTP client (and its
    /// connection pool) instead of creating one — the unified [`LiveKitApi`]
    /// builds one client and shares it across all its services this way.
    ///
    /// [`LiveKitApi`]: super::LiveKitApi
    pub(crate) fn with_client(
        host: &str,
        pkg: &str,
        prefix: Option<&str>,
        client: http_client::Client,
    ) -> Self {
        Self {
            host: normalize_host(host),
            pkg: pkg.to_owned(),
            prefix: prefix.unwrap_or(DEFAULT_PREFIX).to_owned(),
            client,
            failover: FailoverConfig::default(),
            request_timeout: failover::DEFAULT_REQUEST_TIMEOUT,
            #[cfg(test)]
            default_headers: HeaderMap::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_default_headers(mut self, headers: HeaderMap) -> Self {
        self.default_headers = headers;
        self
    }

    /// Enables or disables region failover (enabled by default). Failover only
    /// engages for LiveKit Cloud hosts.
    pub fn with_failover(mut self, enabled: bool) -> Self {
        self.failover.enabled = enabled;
        self
    }

    /// Overrides the default per-attempt request timeout (10s) applied to calls
    /// that don't pass their own. Each failover attempt gets the full budget.
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Overrides the full failover configuration, including the internal
    /// test-only `force` and `backoff_base` knobs.
    #[cfg(test)]
    pub(crate) fn with_failover_config(mut self, config: FailoverConfig) -> Self {
        self.failover = config;
        self
    }

    /// Issues a Twirp request, failing over to alternative regions on retryable
    /// errors. On any transport error or HTTP 5xx it discovers regions via
    /// `/settings/regions` and replays the request — body and headers intact —
    /// against the next untried region, with exponential backoff. A 4xx is
    /// returned immediately.
    pub async fn request<D: prost::Message, R: prost::Message + Default>(
        &self,
        service: &str,
        method: &str,
        data: D,
        headers: HeaderMap,
    ) -> ServerResult<R> {
        self.request_with_timeout(service, method, data, headers, self.request_timeout).await
    }

    /// Like [`request`](Self::request) but with an explicit per-attempt timeout,
    /// for calls (e.g. SIP dialing) that need a longer budget than the default.
    pub async fn request_with_timeout<D: prost::Message, R: prost::Message + Default>(
        &self,
        service: &str,
        method: &str,
        data: D,
        mut headers: HeaderMap,
        timeout: Duration,
    ) -> ServerResult<R> {
        let original = Url::parse(&self.host)?;
        headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
        #[cfg(test)]
        for (k, v) in &self.default_headers {
            headers.insert(k.clone(), v.clone());
        }
        let forward = headers.clone(); // headers for the discovery fetch (no content-type yet)
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/protobuf"));
        let body = data.encode_to_vec();

        let max_attempts = self.failover.attempts(original.host_str(), timeout);
        let mut attempted = vec![failover::host_key(&original)];
        let mut region_urls: Option<Vec<String>> = None;
        let mut current = original.clone();

        for attempt in 0..max_attempts {
            let is_last = attempt + 1 >= max_attempts;
            let url = url_with_path_suffix(
                &current,
                &format!("{}/{}.{}/{}", self.prefix, self.pkg, service, method),
            );

            let send = self
                .client
                .post(url)
                .headers(headers.clone())
                .body(body.clone())
                .timeout(timeout)
                .send()
                .await;
            // The next untried region to fail over to, and a description of the
            // failure for logging. `None` next means give up and surface the error.
            let (next, reason) = match send {
                Ok(resp) => {
                    let status = resp.status();
                    if status == StatusCode::OK {
                        return Ok(R::decode(resp.bytes().await?)?);
                    }
                    // 4xx is terminal; only 5xx is retryable.
                    let next = if is_last || status.as_u16() < 500 {
                        None
                    } else {
                        self.next_region(&original, &forward, &mut region_urls, &attempted).await
                    };
                    // No fallback: surface the server's error (needs the body).
                    let Some(next) = next else {
                        let err: ServerErrorCode = resp.json().await?;
                        return Err(ServerError::Twirp(err));
                    };
                    drop(resp); // release the connection before backing off
                    (next, format!("status {status}"))
                }
                Err(err) => {
                    let next = if is_last {
                        None
                    } else {
                        self.next_region(&original, &forward, &mut region_urls, &attempted).await
                    };
                    match next {
                        Some(next) => (next, err.to_string()),
                        None => return Err(err.into()),
                    }
                }
            };

            log::warn!(
                "livekit API request to {} failed ({}), retrying with fallback url {}",
                current.host_str().unwrap_or_default(),
                reason,
                next,
            );
            failover::backoff_sleep(self.backoff(attempt)).await;
            attempted.push(failover::host_key(&next));
            current = next;
        }
        unreachable!("failover loop always returns within the attempt budget")
    }

    fn backoff(&self, attempt: u32) -> std::time::Duration {
        self.failover.backoff_base * (1u32 << attempt)
    }

    /// Resolves the next untried region, fetching the region list lazily on the
    /// first retryable failure and reusing it thereafter.
    async fn next_region(
        &self,
        original: &Url,
        forward: &HeaderMap,
        region_urls: &mut Option<Vec<String>>,
        attempted: &[String],
    ) -> Option<Url> {
        let region_urls = match region_urls {
            Some(urls) => urls,
            None => region_urls.insert(failover::region_urls(original, forward).await),
        };
        failover::pick_next(region_urls, attempted)
    }
}
