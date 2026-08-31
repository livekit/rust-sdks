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

//! Signal-connection coverage for the non-tokio runtime flavour.
//!
//! The in-crate `signal_test` suite is tokio-only — every case there is a
//! `#[tokio::test]` gated on `signal-client-tokio` — so under `signal-client-async` the
//! signal client is type-checked and never exercised. That gap matters because the parts
//! of the client that differ per runtime are exactly the timing ones:
//! `livekit_runtime::timeout` around the first-message wait, and the keepalive
//! `interval` in `signal_task`. A timeout that never fires under this flavour would be
//! invisible today.
//!
//! Same shape as `services_async.rs`: an integration binary pinned to the non-tokio
//! flavour, driven by `futures::executor::block_on`, run against the shared mock server
//! (`LK_TEST_SERVER_URL`, default `http://127.0.0.1:9999`). It no-ops when the server is
//! unreachable.
//!
//! Only the public surface is visible from here — `SignalInner`, its queue and the mock
//! transport are all private to the crate — so this covers client-observable behaviour,
//! which is the right level for the timing paths anyway.
#![cfg(all(feature = "native-async", not(feature = "native-tokio")))]

use std::time::{Duration, Instant};

use livekit_signaling::{SignalClient, SignalError, SignalOptions};
use livekit_token::{AccessToken, VideoGrants};

/// The mock verifies tokens against this secret by default.
const TEST_SECRET: &str = "secret";
const TEST_API_KEY: &str = "APItest";
const TEST_ROOM: &str = "test-room";
const TEST_IDENTITY: &str = "tester";

/// The attribute key the mock reads its signal-behaviour control object from.
const SIGNAL_CONTROL_ATTRIBUTE: &str = "lk.mock";

/// The mock server's base URL, if it is up.
///
/// `None` means "no server here" — a local checkout without one, where the tests it gates
/// return early instead of failing. Probing the port only, never a response, is deliberate:
/// a server that is up but answering wrongly is a real regression and must fail a test
/// rather than be waved through as "offline". Plain TCP also keeps this file free of an
/// HTTP client, which `access-token` alone does not provide.
fn online_lk_test_server() -> Option<String> {
    let base =
        std::env::var("LK_TEST_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:9999".to_owned());
    let authority = base.split("://").nth(1).unwrap_or(&base).trim_end_matches('/');
    if std::net::TcpStream::connect(authority).is_ok() {
        return Some(base);
    }
    eprintln!("skipping: mock test server not reachable at {base}");
    None
}

/// Mint a token whose `lk.mock` control object selects a server behaviour.
fn token(mode: &str) -> String {
    let mut at = AccessToken::with_api_key(TEST_API_KEY, TEST_SECRET)
        .with_ttl(Duration::from_secs(60 * 60))
        .with_identity(TEST_IDENTITY)
        .with_grants(VideoGrants {
            room_join: true,
            room: TEST_ROOM.to_owned(),
            ..Default::default()
        });
    if !mode.is_empty() {
        let control = format!(r#"{{"signal":"{mode}"}}"#);
        at = at.with_attributes([(SIGNAL_CONTROL_ATTRIBUTE, control.as_str())]);
    }
    at.to_jwt().expect("mint token")
}

/// The connect path end to end on this flavour: WS upgrade, join response, keepalive
/// config. Proves the transport seam and `livekit_runtime::spawn` work here at all.
#[test]
fn signal_async_happy_join() {
    let Some(base) = online_lk_test_server() else { return };

    futures::executor::block_on(async {
        let (client, join, _events) =
            SignalClient::connect(&base, &token(""), SignalOptions::default(), None)
                .await
                .expect("connect must succeed against the mock");

        assert_eq!(join.room.expect("room").name, TEST_ROOM);
        assert!(join.ping_interval > 0, "the mock supplies keepalive config");
        assert!(join.ping_timeout > 0, "the mock supplies keepalive config");
        client.close().await;
    });
}

/// A server that closes before answering is a close, not a timeout. Same assertion as the
/// tokio suite's `close_before_join`, which cannot run on this flavour — and the
/// classification lives in `get_async_message!`, one of the two runtime-sensitive spots.
#[test]
fn signal_async_close_before_join_is_a_close() {
    let Some(base) = online_lk_test_server() else { return };

    futures::executor::block_on(async {
        let err = SignalClient::connect(
            &base,
            &token("close_before_join"),
            SignalOptions::default(),
            None,
        )
        .await
        .err()
        .expect("connect must fail when the server closes before the join");

        assert!(
            matches!(err, SignalError::Closed),
            "a close before the answer is a close, not a timeout, got {err:?}"
        );
    });
}

/// The one that actually tests the runtime: the mock accepts the socket and stays silent,
/// so nothing but `livekit_runtime::timeout` can end the wait. If timers do not drive
/// under this flavour's executor, this hangs rather than fails — which is itself the
/// finding.
#[test]
fn signal_async_no_first_message_times_out() {
    let Some(base) = online_lk_test_server() else { return };

    futures::executor::block_on(async {
        let started = Instant::now();
        let err = SignalClient::connect(
            &base,
            &token("no_first_message"),
            SignalOptions::default(),
            None,
        )
        .await
        .err()
        .expect("connect must fail when the server never answers");

        assert!(
            matches!(err, SignalError::Timeout(_)),
            "a silent server is a timeout, got {err:?}"
        );
        // The wait is the client's own deadline, so it must have actually elapsed —
        // otherwise something else failed the connect and the timer was never proven.
        assert!(
            started.elapsed() >= Duration::from_secs(1),
            "the timeout fired after {:?}, too fast to be the first-message deadline",
            started.elapsed()
        );
    });
}
