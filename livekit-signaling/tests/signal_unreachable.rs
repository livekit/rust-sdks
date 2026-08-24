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

//! The one signal test that needs the *real* network transport, and therefore its own
//! binary.
//!
//! `install_mock_transport()` registers the process-wide `WsClient` in a `OnceLock`:
//! first write wins, and there is no reset. So in any binary where a mock-transport test
//! has run, every later `connect()` succeeds at the transport layer — and a test asserting
//! that an unreachable server is refused gets a connected socket instead, failing with
//! whatever the mock then does. Inside `signal_test` that made the outcome depend on which
//! test happened to reach the transport first: it passed when the mock server was up
//! (the integration tests take seconds, so this one won the race) and failed when it was
//! not. Nothing about ordering or gating fixes that within one binary; a separate binary
//! does, because the mock is never installed here.
#![cfg(feature = "native-tokio")]

use std::time::Duration;

use livekit_signaling::{SignalClient, SignalError, SignalOptions};
use livekit_token::{AccessToken, VideoGrants};

const TEST_SECRET: &str = "secret";
const TEST_API_KEY: &str = "APItest";
const TEST_ROOM: &str = "test-room";
const TEST_IDENTITY: &str = "tester";

fn token() -> String {
    AccessToken::with_api_key(TEST_API_KEY, TEST_SECRET)
        .with_ttl(Duration::from_secs(60 * 60))
        .with_identity(TEST_IDENTITY)
        .with_grants(VideoGrants {
            room_join: true,
            room: TEST_ROOM.to_owned(),
            ..Default::default()
        })
        .to_jwt()
        .expect("mint token")
}

/// A server that isn't listening: the WS connect is refused and the `validate` probe also
/// fails to connect, so the original transport error is surfaced. (client-sdk-js classifies
/// this as `ServerUnreachable`.) Needs no mock server.
#[tokio::test]
async fn unreachable_server_yields_a_connection_error() {
    // Nothing is listening on this port, so the connect is refused immediately.
    let err =
        SignalClient::connect("ws://127.0.0.1:59999", &token(), SignalOptions::default(), None)
            .await
            .err()
            .expect("connecting to a dead port must fail");

    assert!(
        matches!(err, SignalError::Connection(_)),
        "expected a transport connection error for an unreachable server, got {err:?}"
    );
}
