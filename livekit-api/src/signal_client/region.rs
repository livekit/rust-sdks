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

use std::error::Error as StdError;

use http::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::Deserialize;

use crate::http_client;

use super::{SignalError, SignalResult, REGION_FETCH_TIMEOUT};

/// Converts an error into a string that includes the full error chain.
/// This is important for debugging TLS errors, where the root cause
/// (e.g., "invalid peer certificate: UnknownIssuer") is often buried
/// in the source chain.
fn error_with_chain(err: &dyn StdError) -> String {
    std::iter::once(err.to_string())
        .chain(std::iter::successors(err.source(), |err| err.source()).map(ToString::to_string))
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
    pub async fn fetch_region_urls(url: &str, token: &str) -> SignalResult<Vec<String>> {
        if is_cloud_url(url)? {
            let endpoint = region_endpoint(url)?;
            fetch_from_endpoint(&endpoint, token).await
        } else {
            Ok(vec![])
        }
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
        let result = error_with_chain(err);
        assert_eq!(result, "root cause");
    }

    #[test]
    fn test_error_with_chain_two_level_chain() {
        let root =
            RootCauseError { message: "invalid peer certificate: UnknownIssuer".to_string() };
        let middle = MiddleError { message: "error trying to connect".to_string(), source: root };
        let result = error_with_chain(middle);
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
        let result = error_with_chain(outer);
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
        let result = error_with_chain(outer);

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

        let signal_error = SignalError::RegionError(error_with_chain(outer));
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

        let result = error_with_chain(outer);
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
