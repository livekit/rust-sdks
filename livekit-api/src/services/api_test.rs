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

//! API tests against the shared mock LiveKit API server (livekit/livekit
//! cmd/test-server). Point them at a running instance with LK_TEST_SERVER_URL
//! (default http://127.0.0.1:9999); they no-op when no server is reachable. In
//! CI the server is booted as a Docker container.
//!
//! See cmd/test-server/README.md for the X-Lk-Mock-* control protocol. These
//! tests drive TwirpClient::request() directly because the public service
//! methods do not expose per-call headers.

use std::time::Duration;

use http::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use livekit_protocol as proto;

use super::failover::FailoverConfig;
use super::twirp_client::{TwirpClient, TwirpError, TwirpResult};
use super::LIVEKIT_PACKAGE;

fn base_url() -> String {
    std::env::var("LK_TEST_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:9999".to_owned())
}

async fn reachable(base: &str) -> bool {
    reqwest::Client::new()
        .get(format!("{base}/settings/regions"))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

// `force` bypasses the cloud-host check (the mock is on 127.0.0.1) and the tiny
// backoff keeps tests fast — both are internal, test-only knobs.
fn config(enabled: bool, force: bool) -> FailoverConfig {
    FailoverConfig { enabled, force, backoff_base: Duration::from_millis(1) }
}

async fn call(
    base: &str,
    cfg: FailoverConfig,
    directives: &[(&'static str, &str)],
) -> TwirpResult<proto::Room> {
    let client = TwirpClient::new(base, LIVEKIT_PACKAGE, None).with_failover_config(cfg);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer test-token"));
    // These tests exercise failover, not authz; skip the mock's permission check.
    headers
        .insert(HeaderName::from_static("x-lk-mock-skip-auth"), HeaderValue::from_static("true"));
    for (k, v) in directives {
        headers.insert(HeaderName::from_static(k), HeaderValue::from_str(v).unwrap());
    }
    client
        .request::<proto::CreateRoomRequest, proto::Room>(
            "RoomService",
            "CreateRoom",
            proto::CreateRoomRequest::default(),
            headers,
        )
        .await
}

macro_rules! skip_if_offline {
    ($base:expr) => {
        if !reachable(&$base).await {
            eprintln!("skipping: mock test server not reachable at {}", $base);
            return;
        }
    };
}

#[tokio::test]
async fn healthy() {
    let base = base_url();
    skip_if_offline!(base);
    call(&base, config(true, true), &[]).await.expect("healthy request should succeed");
}

#[tokio::test]
async fn primary_unavailable() {
    let base = base_url();
    skip_if_offline!(base);
    call(&base, config(true, true), &[("x-lk-mock-fail-regions", "0")])
        .await
        .expect("should fail over to a healthy region");
}

#[tokio::test]
async fn two_regions_unavailable() {
    let base = base_url();
    skip_if_offline!(base);
    call(&base, config(true, true), &[("x-lk-mock-fail-regions", "0,1")])
        .await
        .expect("should fail over to region 2 on the 3rd attempt");
}

#[tokio::test]
async fn all_unavailable() {
    let base = base_url();
    skip_if_offline!(base);
    let err = call(&base, config(true, true), &[("x-lk-mock-fail-regions", "0,1,2,3")])
        .await
        .expect_err("all regions down should surface an error");
    assert!(matches!(err, TwirpError::Twirp(_)));
}

#[tokio::test]
async fn client_error_not_retried() {
    let base = base_url();
    skip_if_offline!(base);
    let err = call(
        &base,
        config(true, true),
        &[("x-lk-mock-fail-regions", "0"), ("x-lk-mock-fail-status", "400")],
    )
    .await
    .expect_err("a 4xx must be returned without failover");
    match err {
        TwirpError::Twirp(code) => assert_eq!(code.code, "invalid_argument"),
        other => panic!("expected a twirp error, got {other:?}"),
    }
}

#[tokio::test]
async fn transport_error_failover() {
    let base = base_url();
    skip_if_offline!(base);
    call(
        &base,
        config(true, true),
        &[("x-lk-mock-fail-regions", "0"), ("x-lk-mock-fail-mode", "drop")],
    )
    .await
    .expect("a dropped connection should fail over to a healthy region");
}

#[tokio::test]
async fn region_discovery_unreachable() {
    let base = base_url();
    skip_if_offline!(base);
    call(
        &base,
        config(true, true),
        &[("x-lk-mock-fail-regions", "0"), ("x-lk-mock-regions-status", "500")],
    )
    .await
    .expect_err("no fallback hosts means the original 5xx is surfaced");
}

#[tokio::test]
async fn not_cloud_host() {
    let base = base_url();
    skip_if_offline!(base);
    // Enabled but not forced; 127.0.0.1 is not a cloud host, so no failover.
    call(&base, config(true, false), &[("x-lk-mock-fail-regions", "0")])
        .await
        .expect_err("failover should be cloud-gated for a non-cloud host");
}

#[tokio::test]
async fn disabled() {
    let base = base_url();
    skip_if_offline!(base);
    call(&base, config(false, true), &[("x-lk-mock-fail-regions", "0")])
        .await
        .expect_err("disabled failover should not retry");
}
