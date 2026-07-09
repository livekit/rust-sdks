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

//! Signal-connection tests against the shared mock LiveKit server (livekit/livekit
//! cmd/test-server). Point them at a running instance with `LK_TEST_SERVER_URL`
//! (default `http://127.0.0.1:9999`); they no-op when no server is reachable. In
//! CI the server is booted as a Docker container.
//!
//! Unlike the Twirp API tests, signal behavior is not selected by a request
//! header (a WebSocket client cannot set one) but by the `lk.mock` participant
//! attribute embedded in the access token — see cmd/test-server/README.md
//! ("Signal connection (WebSocket) mocking"). Each test mints a token whose
//! `lk.mock` attribute selects a server behavior, then asserts the
//! *client-observable* outcome: how [`SignalClient::connect`] / [`SignalClient::restart`]
//! classify the connection, and what [`SignalEvent`]s reach the caller.
//!
//! These are the Rust counterparts to cmd/test-server/signal_test.go, which
//! exercises the same modes from the server side.

use std::time::Duration;

use livekit_protocol as proto;
use tokio::time::timeout;

use super::{SignalClient, SignalError, SignalEvent, SignalEvents, SignalOptions};
use crate::access_token::{AccessToken, VideoGrants};

/// The mock verifies tokens against this secret by default (matches
/// `livekit-server --dev` and the test-server's `--api-secret` default).
const TEST_SECRET: &str = "secret";
const TEST_API_KEY: &str = "APItest";
const TEST_ROOM: &str = "test-room";
const TEST_IDENTITY: &str = "tester";

/// The attribute key the mock reads its signal-behavior control object from.
const SIGNAL_CONTROL_ATTRIBUTE: &str = "lk.mock";

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

/// Mint a token whose `lk.mock` attribute selects `mode` (empty → no attribute,
/// which the mock treats as the happy path).
fn token(mode: &str) -> String {
    mint(mode, None)
}

/// Mint a token whose `lk.mock` control object carries both a `signal` mode and
/// an explicit `leaveAction` (a `LeaveRequest_Action` enum value).
fn token_with_leave_action(mode: &str, leave_action: proto::leave_request::Action) -> String {
    mint(mode, Some(leave_action as i32))
}

fn mint(mode: &str, leave_action: Option<i32>) -> String {
    let mut at = AccessToken::with_api_key(TEST_API_KEY, TEST_SECRET)
        .with_ttl(Duration::from_secs(60 * 60))
        .with_identity(TEST_IDENTITY)
        .with_grants(VideoGrants {
            room_join: true,
            room: TEST_ROOM.to_owned(),
            ..Default::default()
        });

    if !mode.is_empty() {
        let control = match leave_action {
            Some(action) => format!(r#"{{"signal":"{mode}","leaveAction":{action}}}"#),
            None => format!(r#"{{"signal":"{mode}"}}"#),
        };
        at = at.with_attributes([(SIGNAL_CONTROL_ATTRIBUTE, control.as_str())]);
    }

    at.to_jwt().expect("mint token")
}

fn options(single_peer_connection: bool) -> SignalOptions {
    SignalOptions { single_peer_connection, ..Default::default() }
}

async fn connect(
    base: &str,
    token: &str,
    single_peer_connection: bool,
) -> super::SignalResult<(SignalClient, proto::JoinResponse, SignalEvents)> {
    SignalClient::connect(base, token, options(single_peer_connection), None).await
}

/// Await the next [`SignalEvent`], failing the test if none arrives within `dur`.
async fn next_event(events: &mut SignalEvents, dur: Duration) -> SignalEvent {
    timeout(dur, events.recv())
        .await
        .expect("timed out waiting for a signal event")
        .expect("signal event stream closed unexpectedly")
}

macro_rules! skip_if_offline {
    ($base:expr) => {
        if !reachable(&$base).await {
            eprintln!("skipping: mock test server not reachable at {}", $base);
            return;
        }
    };
}

// -- happy path -------------------------------------------------------------

/// `happy` — the WS sends a `JoinResponse` populated with the room from the
/// token and non-zero ping config (so the client arms keepalive).
#[tokio::test]
async fn happy_join() {
    let base = base_url();
    skip_if_offline!(base);

    let (client, join, _events) =
        connect(&base, &token("happy"), false).await.expect("happy connect should succeed");

    assert_eq!(join.room.as_ref().expect("join.room").name, TEST_ROOM);
    assert!(join.participant.is_some(), "join must carry participant info");
    assert!(join.server_info.is_some(), "join must carry server info");
    assert!(
        join.ping_interval > 0 && join.ping_timeout > 0,
        "keepalive config must be non-zero: interval={} timeout={}",
        join.ping_interval,
        join.ping_timeout
    );

    client.close().await;
}

/// `happy` over the v1 (single-PC) path: `/rtc/v1` behaves identically and still
/// yields a `JoinResponse`.
#[tokio::test]
async fn v1_path_happy() {
    let base = base_url();
    skip_if_offline!(base);

    let (client, join, _events) =
        connect(&base, &token("happy"), true).await.expect("v1 happy connect should succeed");

    assert_eq!(join.room.as_ref().expect("join.room").name, TEST_ROOM);
    assert!(client.is_single_pc_mode_active(), "v1 path should activate single-PC mode");

    client.close().await;
}

/// The client keeps the connection alive: the mock pongs the client's pings, so
/// no `Close` is emitted within a window that exceeds the join's ping timeout.
/// (Mirrors the server-side ping/pong assertions in `TestHappyJoinAndPingPong`.)
#[tokio::test]
async fn happy_stays_connected() {
    let base = base_url();
    skip_if_offline!(base);

    let (client, join, mut events) =
        connect(&base, &token("happy"), false).await.expect("happy connect should succeed");

    // Wait comfortably past the ping timeout: if pongs weren't flowing, the
    // signal task would emit Close("ping timeout") by then.
    let window = Duration::from_secs(join.ping_timeout as u64) + Duration::from_secs(2);
    if let Ok(Some(event)) = timeout(window, events.recv()).await {
        match event {
            SignalEvent::Close(reason) => {
                panic!("connection closed while it should stay alive: {reason}")
            }
            SignalEvent::Message(_) => { /* server-initiated messages are fine */ }
        }
    }

    client.close().await;
}

// -- reconnect --------------------------------------------------------------

/// A `reconnect=1` connection yields a `ReconnectResponse` rather than a join.
/// The client drives this via [`SignalClient::restart`] after an initial connect.
#[tokio::test]
async fn reconnect_response() {
    let base = base_url();
    skip_if_offline!(base);

    let (client, _join, _events) =
        connect(&base, &token("happy"), false).await.expect("initial connect should succeed");

    client.restart().await.expect("restart should yield a ReconnectResponse");

    client.close().await;
}

/// `leave_during_reconnect` — on a `reconnect=1` connection the server sends a
/// `LeaveRequest` first, which the client surfaces as `SignalError::LeaveRequest`
/// from `restart()` (the resume cannot complete). A non-reconnect connection is
/// unaffected, so the initial connect still succeeds.
#[tokio::test]
async fn leave_during_reconnect() {
    let base = base_url();
    skip_if_offline!(base);

    let (client, _join, _events) = connect(&base, &token("leave_during_reconnect"), false)
        .await
        .expect("initial (non-reconnect) connect should succeed");

    let err = client.restart().await.expect_err("restart should surface the server's LeaveRequest");
    match err {
        SignalError::LeaveRequest { reason, action } => {
            assert_eq!(reason, proto::DisconnectReason::ServerShutdown);
            assert_eq!(action, proto::leave_request::Action::Disconnect);
        }
        other => panic!("expected SignalError::LeaveRequest, got {other:?}"),
    }

    client.close().await;
}

// -- post-join disconnects --------------------------------------------------

/// `no_pong` — the server sends the join but never pongs, so the client's
/// keepalive fires and the signal task emits `Close` (ping timeout).
#[tokio::test]
async fn no_pong_times_out() {
    let base = base_url();
    skip_if_offline!(base);

    let (client, join, mut events) = connect(&base, &token("no_pong"), false)
        .await
        .expect("connect should succeed before the ping timeout");

    // Ping timeout is short (join.ping_timeout ~3s); allow some slack.
    let window = Duration::from_secs(join.ping_timeout as u64) + Duration::from_secs(3);
    match next_event(&mut events, window).await {
        // The reason must identify the ping timeout so the engine can classify
        // the disconnect (client-sdk-js asserts the same string).
        SignalEvent::Close(reason) => assert!(
            reason.contains("ping timeout"),
            "expected a ping-timeout close reason, got {reason:?}"
        ),
        SignalEvent::Message(msg) => panic!("expected Close on ping timeout, got message: {msg:?}"),
    }

    client.close().await;
}

/// `close_when_connected` — the server sends the join, then cleanly closes
/// (code 1011). The client observes the stream ending as a `Close` event.
#[tokio::test]
async fn close_when_connected() {
    let base = base_url();
    skip_if_offline!(base);

    let (client, _join, mut events) = connect(&base, &token("close_when_connected"), false)
        .await
        .expect("connect should succeed before the server closes");

    match next_event(&mut events, Duration::from_secs(3)).await {
        SignalEvent::Close(reason) => {
            assert!(!reason.is_empty(), "a transport close should carry a reason")
        }
        SignalEvent::Message(msg) => {
            panic!("expected Close after server close, got message: {msg:?}")
        }
    }

    client.close().await;
}

/// `drop_when_connected` — the server sends the join, then abruptly drops the
/// TCP connection (no close handshake → abnormal 1006). The client still
/// surfaces this as a `Close` event.
#[tokio::test]
async fn drop_when_connected() {
    let base = base_url();
    skip_if_offline!(base);

    let (client, _join, mut events) = connect(&base, &token("drop_when_connected"), false)
        .await
        .expect("connect should succeed before the server drops");

    match next_event(&mut events, Duration::from_secs(3)).await {
        SignalEvent::Close(_) => {}
        SignalEvent::Message(msg) => {
            panic!("expected Close after abrupt drop, got message: {msg:?}")
        }
    }

    client.close().await;
}

/// `leave_when_connected` — the server sends the join, then a `LeaveRequest`.
/// The client forwards it to the caller as a `Message(Leave)` (the engine layer
/// decides how to act on it), carrying `SERVER_SHUTDOWN` and the default
/// `DISCONNECT` action.
#[tokio::test]
async fn leave_when_connected() {
    let base = base_url();
    skip_if_offline!(base);

    let (client, _join, mut events) = connect(&base, &token("leave_when_connected"), false)
        .await
        .expect("connect should succeed before the leave");

    let leave = recv_leave(&mut events).await;
    assert_eq!(leave.reason(), proto::DisconnectReason::ServerShutdown);
    assert_eq!(leave.action(), proto::leave_request::Action::Disconnect);

    client.close().await;
}

/// The `leaveAction` control field overrides the action on emitted leaves.
#[tokio::test]
async fn leave_action_override() {
    let base = base_url();
    skip_if_offline!(base);

    let tok =
        token_with_leave_action("leave_when_connected", proto::leave_request::Action::Reconnect);
    let (client, _join, mut events) =
        connect(&base, &tok, false).await.expect("connect should succeed before the leave");

    let leave = recv_leave(&mut events).await;
    assert_eq!(leave.action(), proto::leave_request::Action::Reconnect);

    client.close().await;
}

/// Read events until a `Leave` message arrives (skipping unrelated server
/// messages, e.g. a token refresh), failing if the stream closes first.
async fn recv_leave(events: &mut SignalEvents) -> proto::LeaveRequest {
    let deadline = Duration::from_secs(3);
    loop {
        match next_event(events, deadline).await {
            SignalEvent::Message(msg) => {
                if let proto::signal_response::Message::Leave(leave) = *msg {
                    return leave;
                }
            }
            SignalEvent::Close(reason) => panic!("stream closed before a Leave arrived: {reason}"),
        }
    }
}

// -- connect-time failures --------------------------------------------------

/// `close_before_join` — the WS upgrade succeeds, then the server closes before
/// sending any first message. The client fails the connect (the stream ended
/// while it was waiting for the join).
#[tokio::test]
async fn close_before_join() {
    let base = base_url();
    skip_if_offline!(base);

    let err = connect(&base, &token("close_before_join"), false)
        .await
        .err()
        .expect("connect must fail when the server closes before the join");
    assert!(
        matches!(err, SignalError::WsError(_)),
        "expected a WS error for a close before join, got {err:?}"
    );
}

/// `no_first_message` — the WS is accepted but the server sends nothing. The
/// client times out waiting for the join.
#[tokio::test]
async fn no_first_message_times_out() {
    let base = base_url();
    skip_if_offline!(base);

    let err = connect(&base, &token("no_first_message"), false)
        .await
        .err()
        .expect("connect must fail when no first message arrives");
    assert!(
        matches!(err, SignalError::Timeout(_)),
        "expected a Timeout waiting for the join, got {err:?}"
    );
}

/// `leave_first_message` on an initial (non-reconnect) connection: the server
/// sends a `LeaveRequest` as the very first message instead of a join, so the
/// connect must be rejected.
///
/// client-sdk-js rejects here by validating the first message and failing fast
/// on the leave. The Rust initial-join path only recognises a `JoinResponse`,
/// so it surfaces the same *outcome* (a rejected connect) but as a
/// `JOIN_RESPONSE` timeout rather than a dedicated leave error — the reconnect
/// path (`get_reconnect_response`) does classify a leave-first as
/// `SignalError::LeaveRequest`; see `leave_during_reconnect`.
#[tokio::test]
async fn leave_first_message_rejects_join() {
    let base = base_url();
    skip_if_offline!(base);

    let err = connect(&base, &token("leave_first_message"), false)
        .await
        .err()
        .expect("connect must be rejected when a leave arrives as the first message");
    assert!(
        matches!(err, SignalError::Timeout(_)),
        "expected the initial join to be rejected (as a timeout), got {err:?}"
    );
}

/// A server that isn't listening: the WS connect is refused and the `validate`
/// probe also fails to connect, so the original transport error is surfaced.
/// (client-sdk-js classifies this as `ServerUnreachable`.) Needs no mock server.
#[tokio::test]
async fn server_unreachable() {
    // Nothing is listening on this port, so the connect is refused immediately.
    let err = connect("ws://127.0.0.1:59999", &token("happy"), false)
        .await
        .err()
        .expect("connecting to a dead port must fail");
    assert!(
        matches!(err, SignalError::WsError(_)),
        "expected a transport (WS) error for an unreachable server, got {err:?}"
    );
}

// -- validate-endpoint error classification ---------------------------------
//
// When the WS upgrade is refused, the client falls back to the `/rtc/validate`
// fetch to obtain a definitive HTTP status/body, and classifies the result as a
// client (4xx) or server (5xx) error. These assert that classification.

/// `validate_500` — WS refused with 500; the `validate` fallback returns 500,
/// which Rust surfaces as `SignalError::Server(500)`.
///
/// DIVERGENCE from client-sdk-js (intentional): there the WS-rejection error
/// shadows the 5xx and the connect is classified as a generic `WebSocket`
/// error — its own comment notes only 401/403/404 override the ws error. Rust's
/// `validate` step exists precisely to recover the real HTTP status (see the
/// issue #1042 fix in `SignalInner::validate`), so preserving the 500 is the
/// more useful behavior; we assert that rather than matching the JS shadowing.
#[tokio::test]
async fn validate_500_is_server_error() {
    let base = base_url();
    skip_if_offline!(base);

    let err = connect(&base, &token("validate_500"), false)
        .await
        .err()
        .expect("a 500 must fail the connect");
    match err {
        SignalError::Server(status, _) => assert_eq!(status.as_u16(), 500),
        other => panic!("expected SignalError::Server(500), got {other:?}"),
    }
}

/// `validate_service_not_found` — 404 without the room marker → client error
/// whose body does NOT contain the "requested room does not exist" marker.
#[tokio::test]
async fn validate_service_not_found_is_client_error() {
    let base = base_url();
    skip_if_offline!(base);

    let err = connect(&base, &token("validate_service_not_found"), false)
        .await
        .err()
        .expect("a 404 must fail the connect");
    match err {
        SignalError::Client(status, body) => {
            assert_eq!(status.as_u16(), 404);
            assert!(
                !body.contains("requested room does not exist"),
                "service-not-found body must not carry the room marker, got {body:?}"
            );
        }
        other => panic!("expected SignalError::Client(404), got {other:?}"),
    }
}

/// `room_not_found` — 404 with the "requested room does not exist" marker.
#[tokio::test]
async fn room_not_found_is_client_error_with_marker() {
    let base = base_url();
    skip_if_offline!(base);

    let err = connect(&base, &token("room_not_found"), false)
        .await
        .err()
        .expect("a 404 must fail the connect");
    match err {
        SignalError::Client(status, body) => {
            assert_eq!(status.as_u16(), 404);
            assert!(
                body.contains("requested room does not exist"),
                "room-not-found body must carry the room marker, got {body:?}"
            );
        }
        other => panic!("expected SignalError::Client(404), got {other:?}"),
    }
}

/// A malformed/unsigned token is rejected: the WS refuses with 401 and the
/// validate fallback confirms it as a client error.
#[tokio::test]
async fn bad_token_is_client_error() {
    let base = base_url();
    skip_if_offline!(base);

    let err =
        connect(&base, "not-a-jwt", false).await.err().expect("a bad token must fail the connect");
    match err {
        SignalError::Client(status, _) => assert_eq!(status.as_u16(), 401),
        // A syntactically invalid bearer may be rejected before the validate
        // round-trip; either classification is a legitimate rejection.
        SignalError::TokenFormat => {}
        other => panic!("expected a 401 client error (or TokenFormat), got {other:?}"),
    }
}
