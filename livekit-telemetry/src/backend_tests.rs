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

//! The backend contract, end to end through the pipeline: what the platform hands over (server
//! URL, token) and what the collector answers, and what the core does about each. One test per
//! row of the contract table in the PR description.

use std::{sync::Arc, time::Duration};

use prost::Message;

use crate::{
    destination::tests::{granted, grantless},
    telemetry::tests::{
        answer, event_names, offline, start, start_cloud, test_config, FakeTransport,
    },
    ExportError, Scope, Telemetry, TelemetryEvent, TelemetryStatus,
};

const PROJECT: &str = "wss://p.livekit.cloud";

/// A Cloud pipeline with one room connected to [`PROJECT`] with a granted token.
fn connected(transport: &Arc<FakeTransport>) -> (Telemetry, Scope) {
    let telemetry = start_cloud(test_config(), transport.clone());
    let room = telemetry.begin_scope();
    room.set_server(PROJECT, &granted(3600));
    (telemetry, room)
}

async fn ping(telemetry: &Telemetry, room: &Scope) {
    room.emit(TelemetryEvent::new("lk.ping"));
    telemetry.flush().await;
}

fn rpc_status(message: &str, retry_secs: Option<i64>) -> Vec<u8> {
    #[derive(Clone, PartialEq, prost::Message)]
    struct Status {
        #[prost(int32, tag = "1")]
        code: i32,
        #[prost(string, tag = "2")]
        message: String,
        #[prost(message, repeated, tag = "3")]
        details: Vec<prost_types::Any>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    struct RetryInfo {
        #[prost(message, optional, tag = "1")]
        retry_delay: Option<prost_types::Duration>,
    }
    let details = retry_secs
        .map(|seconds| prost_types::Any {
            type_url: "type.googleapis.com/google.rpc.RetryInfo".into(),
            value: RetryInfo { retry_delay: Some(prost_types::Duration { seconds, nanos: 0 }) }
                .encode_to_vec(),
        })
        .into_iter()
        .collect();
    Status { code: 8, message: message.into(), details }.encode_to_vec()
}

#[tokio::test(start_paused = true)]
async fn the_ingest_url_and_token_come_from_the_room() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start_cloud(test_config(), transport.clone());
    let room = telemetry.begin_scope();
    let token = granted(3600);
    room.set_server("wss://p.livekit.cloud/rtc?access_token=secret", &token);
    ping(&telemetry, &room).await;
    let sent = transport.sent();
    assert_eq!(sent[0].url, "https://p.livekit.cloud/observability/client/logs/otlp/v0");
    assert_eq!(sent[0].headers["Authorization"], format!("Bearer {token}"));
    assert!(!sent[0].url.contains("secret"), "path and query of the server URL never leak");
}

#[tokio::test(start_paused = true)]
async fn handing_over_the_same_token_again_is_free() {
    let transport = FakeTransport::scripted([]);
    let (telemetry, room) = connected(&transport);
    let token = granted(3600);
    room.set_server(PROJECT, &token);
    for _ in 0..100 {
        room.set_server(PROJECT, &token);
    }
    ping(&telemetry, &room).await;
    assert_eq!(transport.sent().len(), 1, "one upload, no extra wake-ups");
}

#[tokio::test(start_paused = true)]
async fn self_hosted_servers_get_nothing_and_nothing_is_kept() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start_cloud(test_config(), transport.clone());
    let room = telemetry.begin_scope();
    room.set_server("wss://livekit.example.com", &granted(3600));
    ping(&telemetry, &room).await;
    telemetry.emit(TelemetryEvent::new("lk.ping")); // process-level: nobody would take it either
    telemetry.flush().await;
    assert!(transport.sent().is_empty());
    let stats = telemetry.stats();
    assert_eq!(stats.cached_batches, 0, "not collected, so nothing sits on disk");
    assert_eq!(stats.status, TelemetryStatus::Off);
}

#[tokio::test(start_paused = true)]
async fn a_room_without_the_observability_grant_sends_nothing() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start_cloud(test_config(), transport.clone());
    let room = telemetry.begin_scope();
    room.set_server(PROJECT, &grantless(3600));
    ping(&telemetry, &room).await;
    assert!(transport.sent().is_empty(), "no consent, nothing leaves the device");
    assert_eq!(telemetry.stats().cached_batches, 0);
}

#[tokio::test(start_paused = true)]
async fn an_expired_token_holds_uploads_until_a_fresh_one_arrives() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start_cloud(test_config(), transport.clone());
    let room = telemetry.begin_scope();
    room.set_server(PROJECT, &granted(60));
    tokio::time::sleep(Duration::from_secs(61)).await;
    ping(&telemetry, &room).await;
    assert!(transport.sent().is_empty(), "never sent with a token known to be expired");
    assert_eq!(telemetry.stats().status, TelemetryStatus::Waiting);
    assert_eq!(telemetry.stats().dropped, 0, "held, not dropped");

    let fresh = granted(3600);
    room.set_server(PROJECT, &fresh);
    tokio::time::sleep(Duration::from_millis(1)).await;
    let sent = transport.sent();
    assert_eq!(sent.len(), 1, "the refresh releases the backlog right away");
    assert_eq!(sent[0].headers["Authorization"], format!("Bearer {fresh}"));
}

#[tokio::test(start_paused = true)]
async fn a_refresh_that_drops_the_grant_keeps_uploading_with_the_granted_token() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start_cloud(test_config(), transport.clone());
    let room = telemetry.begin_scope();
    let join = granted(3600);
    room.set_server(PROJECT, &join);
    room.set_server(PROJECT, &grantless(7200)); // today's server-side refresh
    ping(&telemetry, &room).await;
    assert_eq!(transport.sent()[0].headers["Authorization"], format!("Bearer {join}"));
}

#[tokio::test(start_paused = true)]
async fn two_rooms_on_two_projects_never_share_a_token_or_a_destination() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start_cloud(test_config(), transport.clone());
    let (a, b) = (telemetry.begin_scope(), telemetry.begin_scope());
    let (token_a, token_b) = (granted(3600), granted(3601));
    a.set_server("wss://a.livekit.cloud", &token_a);
    b.set_server("wss://b.livekit.cloud", &token_b);
    a.emit(TelemetryEvent::new("custom.a"));
    b.emit(TelemetryEvent::new("custom.b"));
    telemetry.flush().await;
    let sent = transport.sent();
    assert_eq!(sent.len(), 2, "one batch per project");
    for request in &sent {
        let names = event_names(request);
        if request.url.starts_with("https://a.") {
            assert_eq!(names, ["custom.a"]);
            assert_eq!(request.headers["Authorization"], format!("Bearer {token_a}"));
        } else {
            assert!(request.url.starts_with("https://b."));
            assert_eq!(names, ["custom.b"]);
            assert_eq!(request.headers["Authorization"], format!("Bearer {token_b}"));
        }
    }
}

#[tokio::test(start_paused = true)]
async fn records_before_the_first_connect_wait_and_go_to_that_project() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start_cloud(test_config(), transport.clone());
    telemetry.emit(TelemetryEvent::new("lk.device.capture.failed")); // before any room
    telemetry.flush().await;
    assert!(transport.sent().is_empty());
    let room = telemetry.begin_scope();
    room.set_server(PROJECT, &granted(3600));
    tokio::time::sleep(Duration::from_millis(1)).await;
    assert_eq!(event_names(&transport.sent()[0]), ["lk.device.capture.failed"]);
}

#[tokio::test(start_paused = true)]
async fn partial_success_counts_the_refused_records_and_never_retries() {
    #[derive(Clone, PartialEq, prost::Message)]
    struct Partial {
        #[prost(int64, tag = "1")]
        rejected: i64,
        #[prost(string, tag = "2")]
        error_message: String,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    struct Response {
        #[prost(message, optional, tag = "1")]
        partial_success: Option<Partial>,
    }
    let body = Response {
        partial_success: Some(Partial { rejected: 2, error_message: "too old".into() }),
    }
    .encode_to_vec();
    let transport = FakeTransport::scripted([answer(200, &[], &body)]);
    let (telemetry, room) = connected(&transport);
    for _ in 0..3 {
        room.emit(TelemetryEvent::new("lk.ping"));
    }
    telemetry.flush().await;
    let stats = telemetry.stats();
    assert_eq!((stats.uploads_sent, stats.dropped_rejected), (1, 2));
    assert_eq!(stats.cached_batches, 0, "accepted: never sent again");
}

#[tokio::test(start_paused = true)]
async fn a_bad_request_drops_the_batch() {
    for status in [400, 422] {
        let transport = FakeTransport::scripted([answer(status, &[], b"nope")]);
        let (telemetry, room) = connected(&transport);
        ping(&telemetry, &room).await;
        ping(&telemetry, &room).await;
        let stats = telemetry.stats();
        assert_eq!(stats.dropped_rejected, 1, "{status}: dropped, counted");
        // The second flush: the ping, and (its own owner) the report of the loss.
        assert_eq!(transport.sent().len(), 3, "{status}: not retried, uploads carry on");
    }
}

#[tokio::test(start_paused = true)]
async fn disabled_project_goes_silent_and_purges_its_cache() {
    let disabled = answer(401, &[], b"project data recording is disabled by owner");
    let transport = FakeTransport::scripted([disabled]);
    let telemetry = start_cloud(test_config(), transport.clone());
    let (a, b) = (telemetry.begin_scope(), telemetry.begin_scope());
    a.set_server("wss://a.livekit.cloud", &granted(3600));
    a.emit(TelemetryEvent::new("custom.a"));
    telemetry.flush().await;
    a.emit(TelemetryEvent::new("custom.a"));
    telemetry.flush().await;
    assert_eq!(transport.sent().len(), 1, "never sent again");
    assert_eq!(telemetry.stats().dropped_disabled, 2, "the batch and what followed");
    assert_eq!(telemetry.stats().cached_batches, 0);

    b.set_server("wss://b.livekit.cloud", &granted(3600));
    b.emit(TelemetryEvent::new("custom.b"));
    telemetry.flush().await;
    assert_eq!(event_names(&transport.sent()[1]), ["custom.b"], "another project is unaffected");
}

#[tokio::test(start_paused = true)]
async fn unauthorized_holds_the_batch_until_a_new_token() {
    let transport = FakeTransport::scripted([answer(401, &[], b"invalid token")]);
    let (telemetry, room) = connected(&transport);
    ping(&telemetry, &room).await;
    ping(&telemetry, &room).await;
    assert_eq!(transport.sent().len(), 1, "the refused token is not tried again");
    let stats = telemetry.stats();
    // Two pings, plus the report of the refusal (process-level: its own batch, same token).
    assert_eq!((stats.dropped, stats.cached_batches), (0, 3), "a credential problem loses nothing");
    assert_eq!(stats.status, TelemetryStatus::Waiting);

    room.set_server(PROJECT, &granted(7200));
    tokio::time::sleep(secs(5)).await;
    assert_eq!(telemetry.stats().cached_batches, 0, "the new token ships the backlog");
}

#[tokio::test(start_paused = true)]
async fn not_found_on_the_derived_endpoint_goes_silent() {
    let transport = FakeTransport::scripted([answer(404, &[], b"")]);
    let (telemetry, room) = connected(&transport);
    ping(&telemetry, &room).await;
    ping(&telemetry, &room).await;
    assert_eq!(transport.sent().len(), 1);
    assert_eq!(telemetry.stats().status, TelemetryStatus::Off);
    assert_eq!(telemetry.stats().cached_batches, 0);
}

#[tokio::test(start_paused = true)]
async fn throttling_honors_retry_after_then_retry_info_then_a_minute() {
    let cases: [(crate::ExportResponse, Duration); 4] = [
        (answer(429, &[("Retry-After", "7")], &rpc_status("q", Some(30))).unwrap(), secs(7)),
        (answer(429, &[], &rpc_status("q", Some(30))).unwrap(), secs(30)),
        (answer(429, &[], &rpc_status("QuotaStatusExceeded", None)).unwrap(), secs(60)),
        (answer(503, &[("Retry-After", "12")], b"").unwrap(), secs(12)),
    ];
    for (response, wait) in cases {
        let transport = FakeTransport::scripted([Ok(response.clone())]);
        let (telemetry, room) = connected(&transport);
        ping(&telemetry, &room).await;
        assert_eq!(telemetry.stats().status, TelemetryStatus::Throttled);
        ping(&telemetry, &room).await; // collection goes on during the pause
        tokio::time::sleep(wait - Duration::from_millis(10)).await;
        assert_eq!(transport.sent().len(), 1, "{}: quiet for {wait:?}", response.status);
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(transport.sent().len() >= 3, "{}: both batches ship after", response.status);
        assert_eq!(telemetry.stats().dropped, 0);
    }
}

#[tokio::test(start_paused = true)]
async fn a_retryable_500_waits_for_its_retry_info() {
    let transport = FakeTransport::scripted([answer(500, &[], &rpc_status("later", Some(9)))]);
    let (telemetry, room) = connected(&transport);
    ping(&telemetry, &room).await;
    tokio::time::sleep(secs(8)).await;
    assert_eq!(transport.sent().len(), 1);
    tokio::time::sleep(secs(2)).await;
    assert_eq!(transport.sent().len(), 2, "retried after the delay the server named");
    assert_eq!(telemetry.stats().cached_batches, 0);
}

#[tokio::test(start_paused = true)]
async fn other_server_errors_drop_the_batch() {
    for status in [500, 501, 505] {
        let transport = FakeTransport::scripted([answer(status, &[], b"boom")]);
        let (telemetry, room) = connected(&transport);
        ping(&telemetry, &room).await;
        assert_eq!(telemetry.stats().dropped_rejected, 1, "{status}: final per OTLP/HTTP");
    }
}

/// 502/503/504 without a delay: jittered backoff, and running out of patience pauses — it never
/// deletes. Only the cache's age and size bound what is kept.
#[tokio::test(start_paused = true)]
async fn a_failing_server_never_costs_a_batch() {
    let failures = [502, 503, 504].into_iter().cycle().take(12).map(|s| answer(s, &[], b""));
    let transport = FakeTransport::scripted(failures);
    let (telemetry, room) = connected(&transport);
    ping(&telemetry, &room).await;
    tokio::time::sleep(secs(20 * 60)).await;
    let stats = telemetry.stats();
    assert_eq!(transport.sent().len(), 13, "twelve failures, then delivered");
    assert_eq!((stats.dropped, stats.cached_batches, stats.upload_failures), (0, 0, 12));
}

/// No answer at all (offline, DNS, TLS, connection reset, timeout): 1 s doubling to a 60 s cap,
/// fully jittered, never dropped.
#[tokio::test(start_paused = true)]
async fn no_answer_backs_off_exponentially_with_full_jitter() {
    let transport = FakeTransport::scripted(std::iter::repeat_with(offline).take(8));
    let (telemetry, room) = connected(&transport);
    ping(&telemetry, &room).await;
    let mut at = Vec::new();
    let start = tokio::time::Instant::now();
    while transport.sent().len() < 9 {
        let before = transport.sent().len();
        tokio::time::sleep(Duration::from_millis(10)).await;
        if transport.sent().len() > before {
            at.push(start.elapsed().as_secs_f64());
        }
    }
    // The flush tick (1 s here) retries too once a wait is over; waits never exceed the backoff.
    let gaps: Vec<f64> = std::iter::once(at[0]).chain(at.windows(2).map(|w| w[1] - w[0])).collect();
    for (gap, full) in gaps.iter().zip([1.0_f64, 2.0, 4.0, 8.0, 16.0, 32.0, 60.0, 60.0]) {
        assert!(*gap <= full.max(1.0) + 1.05, "gap {gap} over {full} ({gaps:?})");
    }
    let stats = telemetry.stats();
    assert_eq!((stats.dropped, stats.cached_batches), (0, 0), "delivered in the end, none lost");
}

#[tokio::test(start_paused = true)]
async fn an_invalid_request_is_dropped() {
    let invalid = Err(ExportError::Rejected { reason: "invalid URL".into() });
    let transport = FakeTransport::scripted([invalid]);
    let (telemetry, room) = connected(&transport);
    ping(&telemetry, &room).await;
    assert_eq!(telemetry.stats().dropped_rejected, 1);
}

#[tokio::test(start_paused = true)]
async fn the_override_reaches_a_local_collector_without_cloud_rules() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start(test_config(), transport.clone());
    let room = telemetry.begin_scope();
    room.set_server("ws://localhost:7880", "dev-token-without-grant");
    ping(&telemetry, &room).await;
    let sent = transport.sent();
    assert_eq!(sent[0].url, "http://collector/v1/logs");
    assert!(!sent[0].headers.contains_key("Authorization"), "tokens never go to an override");
}

/// A real HTTP exchange: a mock OTLP collector on a local socket answers 429 with `Retry-After`,
/// then 200, through the `livekit-net` HTTP stack the SDKs can use.
#[cfg(feature = "net")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_real_http_collector_throttles_and_recovers() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let (seen_tx, mut seen) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        let answers = [
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
        ];
        for answer in answers.iter().cycle() {
            let Ok((mut socket, _)) = listener.accept().await else { return };
            let mut request = Vec::new();
            let mut chunk = [0u8; 4096];
            // Read the head, then exactly Content-Length bytes of body.
            loop {
                let n = socket.read(&mut chunk).await.unwrap_or(0);
                request.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&request).to_string();
                if let Some(head_end) = text.find("\r\n\r\n") {
                    let length = text[..head_end]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if request.len() >= head_end + 4 + length || n == 0 {
                        let _ = seen_tx.send(text[..head_end].to_owned());
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let _ = socket.write_all(answer.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });

    let transport = Arc::new(crate::NetTransport::new(livekit_net::testing::native_http_client()));
    let (telemetry, exporter) = Telemetry::new(test_config(), transport);
    telemetry.override_endpoint(&format!("http://{addr}"));
    tokio::spawn(exporter.run());
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    telemetry.flush().await;
    let head = seen.recv().await.expect("first request");
    assert!(head.starts_with("POST /v1/logs"), "{head}");
    assert!(head.to_ascii_lowercase().contains("content-encoding: gzip"));
    assert_eq!(telemetry.stats().status, TelemetryStatus::Throttled);

    tokio::time::timeout(Duration::from_secs(5), seen.recv()).await.expect("retried").expect("req");
    tokio::time::sleep(Duration::from_millis(200)).await;
    let stats = telemetry.stats();
    assert_eq!((stats.uploads_sent, stats.dropped, stats.cached_batches), (1, 0, 0));
}

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

/// 413: the batch is split in half and retried at once, down to single records; a record too
/// big on its own is dropped and counted. The halves are committed before the original goes.
#[tokio::test(start_paused = true)]
async fn payload_too_large_splits_down_to_single_records() {
    let too_large = || answer(413, &[], b"");
    // 4 records: 413 → [2, 2]; first half 413 → [1, 1]; the first single 413 → oversized.
    let transport = FakeTransport::scripted([too_large(), too_large(), too_large()]);
    let (telemetry, room) = connected(&transport);
    for n in 0..4 {
        room.emit(TelemetryEvent::new(format!("custom.e{n}")));
    }
    telemetry.flush().await;
    let delivered: Vec<String> = transport.sent()[3..].iter().flat_map(event_names).collect();
    assert_eq!(delivered, ["custom.e1", "custom.e2", "custom.e3"], "in order, e0 alone too big");
    let stats = telemetry.stats();
    assert_eq!((stats.dropped_oversized, stats.dropped, stats.cached_batches), (1, 1, 0));
}

/// Repeated 401s while the renewal is pending: every refused token is tried once, never again,
/// and the batch waits through all of them.
#[tokio::test(start_paused = true)]
async fn repeated_unauthorized_answers_wait_for_renewal_without_loss() {
    let denied = || answer(401, &[], b"invalid token");
    let transport = FakeTransport::scripted([denied(), denied()]);
    let (telemetry, room) = connected(&transport);
    ping(&telemetry, &room).await;
    room.set_server(PROJECT, &granted(3601));
    tokio::time::sleep(Duration::from_millis(1)).await;
    assert_eq!(transport.sent().len(), 2, "the second token is refused too");
    for _ in 0..5 {
        ping(&telemetry, &room).await;
    }
    assert_eq!(transport.sent().len(), 2, "no token left to try: nothing is sent");
    room.set_server(PROJECT, &granted(3602));
    tokio::time::sleep(secs(5)).await;
    let stats = telemetry.stats();
    assert_eq!((stats.dropped, stats.cached_batches, stats.uploads_unauthorized), (0, 0, 2));
}

/// A server-directed delay is honored in full: a shutdown drains everything else, but not
/// through a `Retry-After`.
#[tokio::test(start_paused = true)]
async fn shutdown_never_cuts_a_server_delay_short() {
    let transport = FakeTransport::scripted([answer(503, &[("Retry-After", "600")], b"")]);
    let (telemetry, room) = connected(&transport);
    ping(&telemetry, &room).await;
    ping(&telemetry, &room).await;
    telemetry.shutdown().await;
    assert_eq!(transport.sent().len(), 1, "no request before the ten minutes are up");
}

/// An expired token is a hard hold: the soft holds' one-minute escape never sends with it.
#[tokio::test(start_paused = true)]
async fn hard_holds_have_no_escape_hatch() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start_cloud(test_config(), transport.clone());
    let room = telemetry.begin_scope();
    room.set_server(PROJECT, &granted(10));
    let _connecting = room.start(crate::SpanName::Connect, None); // a soft hold on top
    tokio::time::sleep(secs(11)).await;
    for _ in 0..10 {
        ping(&telemetry, &room).await;
        tokio::time::sleep(secs(61)).await;
    }
    assert!(transport.sent().is_empty(), "ten minutes, not a single request");
    assert_eq!(telemetry.stats().dropped, 0);
}

/// After a restart the cached batches wait for a token of their own project — never another
/// project's — bounded by the cache's 24 h age.
#[tokio::test(start_paused = true)]
async fn a_restart_with_cached_data_and_no_token_waits_for_the_same_project() {
    let dir = crate::cache::temp_dir("restart");
    let config = || crate::TelemetryConfig {
        storage_dir: Some(dir.to_string_lossy().into_owned()),
        ..test_config()
    };
    let first =
        start_cloud(config(), FakeTransport::scripted(std::iter::repeat_with(offline).take(64)));
    let room = first.begin_scope();
    room.set_server("wss://a.livekit.cloud", &granted(3600));
    room.emit(TelemetryEvent::new("custom.yesterday"));
    first.flush().await; // offline: stays on disk
    first.shutdown().await; // the first launch is over: its exporter has stopped
    drop((room, first));

    let transport = FakeTransport::scripted([]);
    let second = start_cloud(config(), transport.clone());
    second.flush().await;
    assert!(transport.sent().is_empty(), "no token yet");
    let other = second.begin_scope();
    other.set_server("wss://b.livekit.cloud", &granted(3600));
    other.emit(TelemetryEvent::new("custom.today"));
    second.flush().await;
    let urls: Vec<String> = transport.sent().iter().map(|r| r.url.clone()).collect();
    assert!(urls.iter().all(|u| u.starts_with("https://b.")), "b's token never carries a's data");
    assert!(!transport.sent().iter().flat_map(event_names).any(|n| n == "custom.yesterday"));

    let again = second.begin_scope();
    again.set_server("wss://a.livekit.cloud", &granted(3600));
    tokio::time::sleep(Duration::from_millis(1)).await;
    let last = transport.sent().pop().expect("sent");
    assert!(last.url.starts_with("https://a.") && event_names(&last) == ["custom.yesterday"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Finding r1-2: records keep the project they were captured for. A batch cached for project a
/// still goes to a with a's token after the Room reconnects to b; records queued before the
/// switch do too.
#[tokio::test(start_paused = true)]
async fn a_room_switching_projects_takes_nothing_along() {
    let transport = FakeTransport::scripted([offline()]);
    let telemetry = start_cloud(test_config(), transport.clone());
    let room = telemetry.begin_scope();
    let (a, b) = (granted(3600), granted(3601));
    room.set_server("wss://a.livekit.cloud", &a);
    room.emit(TelemetryEvent::new("custom.cached_for_a"));
    telemetry.flush().await; // offline: cached for a
    room.emit(TelemetryEvent::new("custom.queued_for_a")); // captured for a, not yet encoded
    room.set_server("wss://b.livekit.cloud", &b);
    room.emit(TelemetryEvent::new("custom.for_b"));
    tokio::time::sleep(secs(70)).await;
    let sent = transport.sent();
    for request in &sent[1..] {
        let names = event_names(request);
        let (url, auth) = (&request.url, &request.headers["Authorization"]);
        if names.iter().any(|n| n.ends_with("_for_a")) {
            assert!(url.starts_with("https://a.") && *auth == format!("Bearer {a}"), "{names:?}");
            assert!(!names.contains(&"custom.for_b".to_owned()));
        }
        if names.contains(&"custom.for_b".to_owned()) {
            assert!(url.starts_with("https://b.") && *auth == format!("Bearer {b}"));
        }
    }
    let all: Vec<String> = sent[1..].iter().flat_map(event_names).collect();
    for name in ["custom.cached_for_a", "custom.queued_for_a", "custom.for_b"] {
        assert!(all.contains(&name.to_owned()), "{name} delivered: {all:?}");
    }
}

/// Finding r1-2: a Room that has no server yet never has its records sent to another Room's
/// project; they go to its own first project once it connects.
#[tokio::test(start_paused = true)]
async fn an_unconnected_room_never_borrows_another_rooms_project() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start_cloud(test_config(), transport.clone());
    let (early, connected) = (telemetry.begin_scope(), telemetry.begin_scope());
    early.emit(TelemetryEvent::new("custom.early"));
    connected.set_server("wss://a.livekit.cloud", &granted(3600));
    connected.emit(TelemetryEvent::new("custom.a"));
    telemetry.flush().await;
    tokio::time::sleep(secs(5)).await;
    let names: Vec<String> = transport.sent().iter().flat_map(event_names).collect();
    assert!(!names.contains(&"custom.early".to_owned()), "not on a's token: {names:?}");

    early.set_server("wss://b.livekit.cloud", &granted(3601));
    tokio::time::sleep(secs(5)).await;
    let last =
        transport.sent().into_iter().find(|r| event_names(r).contains(&"custom.early".to_owned()));
    assert!(last.is_some_and(|r| r.url.starts_with("https://b.")), "its own project");
}

/// One project's failures pause only that project: the other keeps uploading.
#[tokio::test(start_paused = true)]
async fn a_failing_project_does_not_pause_the_others() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start_cloud(test_config(), transport.clone());
    let (a, b) = (telemetry.begin_scope(), telemetry.begin_scope());
    a.set_server("wss://a.livekit.cloud", &granted(3600));
    a.emit(TelemetryEvent::new("custom.a"));
    telemetry.flush().await; // a's first batch goes out and is accepted
    transport.then(std::iter::repeat_with(|| answer(503, &[], b"")).take(1));
    a.emit(TelemetryEvent::new("custom.a"));
    telemetry.flush().await; // a now backs off
    b.set_server("wss://b.livekit.cloud", &granted(3600));
    b.emit(TelemetryEvent::new("custom.b"));
    telemetry.flush().await;
    let last = transport.sent().pop().expect("sent");
    assert!(last.url.starts_with("https://b."), "b is not held by a's backoff");
}

/// A reconnect handing over the same URL and token retries an ingest that answered 404.
#[tokio::test(start_paused = true)]
async fn a_reconnect_with_the_same_token_retries_a_404() {
    let transport = FakeTransport::scripted([answer(404, &[], b"")]);
    let token = granted(3600);
    let telemetry = start_cloud(test_config(), transport.clone());
    let room = telemetry.begin_scope();
    room.set_server(PROJECT, &token);
    ping(&telemetry, &room).await;
    assert_eq!(telemetry.stats().status, TelemetryStatus::Off);
    room.set_server(PROJECT, &token); // the reconnect: same pair
    ping(&telemetry, &room).await;
    assert_eq!(transport.sent().len(), 2, "tried again");
    assert_eq!(telemetry.stats().status, TelemetryStatus::Ok);
}

/// A one-connection-at-a-time HTTP server answering every request with `answer`, reporting each
/// request head it saw.
#[cfg(feature = "net")]
async fn http_server(
    answer: String,
) -> (std::net::SocketAddr, tokio::sync::mpsc::UnboundedReceiver<String>) {
    http_server_by_host(move |_| answer.clone()).await
}

/// Like [`http_server`], answering each request by its `Host` header.
#[cfg(feature = "net")]
async fn http_server_by_host(
    answer: impl Fn(&str) -> String + Send + 'static,
) -> (std::net::SocketAddr, tokio::sync::mpsc::UnboundedReceiver<String>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let (seen_tx, seen) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else { return };
            let mut request = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = socket.read(&mut chunk).await.unwrap_or(0);
                request.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&request).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        let _ = seen_tx.send(text[..end].to_owned());
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&request).to_string();
            let host = text
                .lines()
                .find_map(|l| l.strip_prefix("host: ").or_else(|| l.strip_prefix("Host: ")))
                .unwrap_or_default()
                .to_owned();
            let _ = socket.write_all(answer(&host).as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });
    (addr, seen)
}

/// The transport rule for credentials: a redirect to another port, or to another host on the
/// same port, never carries the token. (A scheme-only change on the same host and explicit port
/// is not covered here: it needs a TLS origin; the derived Cloud URLs are always `https` on the
/// default port.)
#[cfg(feature = "net")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn redirects_to_another_host_or_port_never_carry_the_token() {
    use crate::TelemetryTransport;
    let ok = "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 0\r\n\r\n".to_owned();
    let (target, mut seen) = http_server(ok).await;
    for location in [
        format!("http://127.0.0.1:{}/v1/logs", target.port()), // another port
        format!("http://localhost:{}/v1/logs", target.port()), // another host
    ] {
        let redirect = format!(
            "HTTP/1.1 307 Temporary Redirect\r\nLocation: {location}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
        );
        let (origin, _) = http_server(redirect).await;
        let transport = crate::NetTransport::new(livekit_net::testing::native_http_client());
        let request = crate::ExportRequest {
            url: format!("http://127.0.0.1:{}/v1/logs", origin.port()),
            headers: [("Authorization".to_owned(), "Bearer secret".to_owned())].into(),
            body: vec![1, 2, 3],
        };
        let _ = transport.send(request).await;
        let head = tokio::time::timeout(Duration::from_secs(5), seen.recv())
            .await
            .expect("followed")
            .expect("head");
        assert!(!head.to_ascii_lowercase().contains("authorization"), "{location}: {head}");
    }

    // Same port, another host: `localhost:<port>` redirects to `127.0.0.1:<port>`.
    let (server, mut heads) = http_server_by_host(|host| {
        if host.starts_with("localhost") {
            let port = host.rsplit(':').next().unwrap_or_default();
            format!(
                "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://127.0.0.1:{port}/v1/logs\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
            )
        } else {
            "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 0\r\n\r\n".to_owned()
        }
    })
    .await;
    let transport = crate::NetTransport::new(livekit_net::testing::native_http_client());
    let request = crate::ExportRequest {
        url: format!("http://localhost:{}/v1/logs", server.port()),
        headers: [("Authorization".to_owned(), "Bearer secret".to_owned())].into(),
        body: vec![1, 2, 3],
    };
    let _ = transport.send(request).await;
    let first = heads.recv().await.expect("the origin request");
    assert!(first.to_ascii_lowercase().contains("authorization"), "sent to the origin");
    let second = tokio::time::timeout(Duration::from_secs(5), heads.recv())
        .await
        .expect("followed")
        .expect("head");
    assert!(!second.to_ascii_lowercase().contains("authorization"), "{second}");
}

/// Finding r2-1: the answer to a batch a Room captured before it connected is attributed to the
/// project the batch was sent to — that Room's first project — never to the latest project: a
/// 404 or a 429 from a leaves b alone.
#[tokio::test(start_paused = true)]
async fn answers_to_pre_connect_batches_are_attributed_to_their_own_project() {
    for (answer_a, what) in [(answer(404, &[], b""), "404"), (answer(429, &[], b""), "429")] {
        let transport = FakeTransport::scripted([answer_a]);
        let telemetry = start_cloud(test_config(), transport.clone());
        let (early, other) = (telemetry.begin_scope(), telemetry.begin_scope());
        early.emit(TelemetryEvent::new("custom.before_connect")); // no project yet
        early.set_server("wss://a.livekit.cloud", &granted(3600));
        other.set_server("wss://b.livekit.cloud", &granted(3601)); // b is now the latest
        telemetry.flush().await;
        assert!(transport.sent()[0].url.starts_with("https://a."), "{what}: sent to a");
        other.emit(TelemetryEvent::new("custom.b"));
        telemetry.flush().await;
        let to_b = transport.sent().into_iter().find(|r| r.url.starts_with("https://b."));
        assert!(to_b.is_some_and(|r| event_names(&r) == ["custom.b"]), "{what}: b unaffected");
    }
}

/// Finding r3-3: a Room that captured records before it connected, then went away before they
/// were sent, still gets them sent with its first project's credential.
#[tokio::test(start_paused = true)]
async fn a_gone_rooms_pre_connect_backlog_keeps_its_credential() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start_cloud(test_config(), transport.clone());
    let offline =
        crate::DeviceState { network: crate::NetworkType::Unavailable, ..Default::default() };
    telemetry.set_device_state(offline);
    let room = telemetry.begin_scope();
    room.emit(TelemetryEvent::new("custom.before_connect")); // captured without a project
    room.set_server("wss://a.livekit.cloud", &granted(3600));
    telemetry.flush().await; // cached, host-less; offline: not sent
    drop(room); // the Room goes away (its connect failed)
    for _ in 0..3 {
        telemetry.flush().await; // passes retire what nothing needs any more
    }
    telemetry.set_device_state(crate::DeviceState::default());
    tokio::time::sleep(secs(2)).await;
    let sent = transport
        .sent()
        .into_iter()
        .find(|r| event_names(r).contains(&"custom.before_connect".to_owned()));
    assert!(sent.is_some_and(|r| r.url.starts_with("https://a.")), "sent with a's credential");
}

/// Codex final review B2: what a Room captured before its first `set_server` — cached before
/// the connect, or encoded after it — names that Room's project on disk, so it replays after a
/// restart with a token of the same project.
#[tokio::test(start_paused = true)]
async fn pre_connect_records_replay_after_a_restart() {
    use crate::{DeviceState, NetworkType, TelemetryConfig};
    let dir = crate::cache::temp_dir("pre-connect-restart");
    let config = || TelemetryConfig {
        storage_dir: Some(dir.to_string_lossy().into_owned()),
        ..test_config()
    };
    let (first, exporter) = Telemetry::new(config(), FakeTransport::scripted([]));
    let task = tokio::spawn(exporter.run());
    first.set_device_state(DeviceState { network: NetworkType::Unavailable, ..Default::default() });
    let room = first.begin_scope();
    room.emit(TelemetryEvent::new("custom.cached_before"));
    first.flush().await; // cached without a project
    room.emit(TelemetryEvent::new("custom.encoded_after")); // captured without a project
    room.set_server("wss://a.livekit.cloud", &granted(3600));
    first.flush().await; // offline: nothing sent
    task.abort(); // killed before going online
    let _ = task.await;
    drop((room, first));

    let transport = FakeTransport::scripted([]);
    let second = start_cloud(config(), transport.clone());
    let again = second.begin_scope();
    again.set_server("wss://a.livekit.cloud", &granted(3601));
    second.flush().await;
    let to_a: Vec<String> = transport
        .sent()
        .iter()
        .filter(|r| r.url.starts_with("https://a."))
        .flat_map(event_names)
        .collect();
    for name in ["custom.cached_before", "custom.encoded_after"] {
        assert!(to_a.contains(&name.to_owned()), "{name} replayed to a: {to_a:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Whether a journaled rewrite (`<parent>@<half>.pend`) is in progress in `dir`.
fn rewriting(dir: &std::path::Path) -> bool {
    std::fs::read_dir(dir)
        .map(|d| d.flatten().any(|e| e.path().extension().is_some_and(|x| x == "pend")))
        .unwrap_or(false)
}

/// Final review r2 B2a: the process dies while a pre-connect batch is being bound to its Room's
/// project — once the bound copy is journaled, before or after the commit (the parent's delete) —
/// and a fresh pipeline still replays it to that project, exactly once.
#[tokio::test(start_paused = true)]
async fn a_crash_while_binding_still_replays_to_the_rooms_project() {
    use crate::{
        cache::{FileCache, Step},
        DeviceState, NetworkType, TelemetryConfig,
    };
    for crash_at in [Step::Delete, Step::Publish] {
        let dir = crate::cache::temp_dir("bind-crash");
        let config = || TelemetryConfig {
            storage_dir: Some(dir.to_string_lossy().into_owned()),
            ..test_config()
        };
        let cache = Arc::new(FileCache::open(&dir, 1 << 20).expect("cache"));
        let (first, exporter) =
            Telemetry::with_cache(config(), FakeTransport::scripted([]), cache.clone());
        let task = tokio::spawn(exporter.run());
        first.set_device_state(DeviceState {
            network: NetworkType::Unavailable,
            ..Default::default()
        });
        let room = first.begin_scope();
        room.emit(TelemetryEvent::new("custom.cached_before"));
        first.flush().await; // cached without a project
        let journal = dir.clone();
        cache.inject(move |step, _| {
            assert!(!(step == crash_at && rewriting(&journal)), "killed while binding");
            Ok(())
        });
        room.set_server("wss://a.livekit.cloud", &granted(3600));
        let _ = tokio::time::timeout(Duration::from_secs(5), first.flush()).await;
        assert!(task.await.is_err(), "{crash_at:?}: killed mid-bind");
        assert!(rewriting(&dir), "{crash_at:?}: the bound copy is journaled");
        drop((room, first));

        let transport = FakeTransport::scripted([]);
        let second = start_cloud(config(), transport.clone());
        let again = second.begin_scope();
        again.set_server("wss://a.livekit.cloud", &granted(3601));
        second.flush().await;
        let to_a: Vec<String> = transport
            .sent()
            .iter()
            .filter(|r| r.url.starts_with("https://a."))
            .flat_map(event_names)
            .filter(|name| name == "custom.cached_before")
            .collect();
        assert_eq!(to_a.len(), 1, "{crash_at:?}: replayed to a once");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Final review r2 B2b: a pre-connect batch whose binding failed is uploaded through the live
/// Room → project map, accepted, and its delete keeps failing; a later chance to bind it must not
/// give it a new id that is sent again.
#[tokio::test(start_paused = true)]
async fn an_accepted_batch_awaiting_its_delete_is_not_rebound_and_resent() {
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

    use crate::{
        cache::{FileCache, Step},
        BatchCache, DeviceState, NetworkType,
    };
    let dir = crate::cache::temp_dir("bind-undeletable");
    let cache = Arc::new(FileCache::open(&dir, 1 << 20).expect("cache"));
    let transport = FakeTransport::scripted([]);
    let (telemetry, exporter) =
        Telemetry::with_cache(test_config(), transport.clone(), cache.clone());
    tokio::spawn(exporter.run());
    telemetry
        .set_device_state(DeviceState { network: NetworkType::Unavailable, ..Default::default() });
    let room = telemetry.begin_scope();
    room.emit(TelemetryEvent::new("custom.once"));
    telemetry.flush().await; // cached without a project
    let (bind_fails, delete_fails) =
        (Arc::new(AtomicBool::new(true)), Arc::new(AtomicBool::new(true)));
    let (binds, deletes, journal) = (bind_fails.clone(), delete_fails.clone(), dir.clone());
    cache.inject(move |step, path| {
        let bound = path.to_string_lossy().contains("a.livekit.cloud");
        match step {
            Step::Write if bound && binds.load(SeqCst) => Err(std::io::Error::other("bind")),
            // A plain delete fails; a rewrite's commit (its journal written) goes through.
            Step::Delete if !rewriting(&journal) && deletes.load(SeqCst) => {
                Err(std::io::Error::other("delete"))
            }
            _ => Ok(()),
        }
    });
    room.set_server("wss://a.livekit.cloud", &granted(3600));
    telemetry.set_device_state(DeviceState::default());
    telemetry.flush().await; // binding fails; sent through the live map, accepted; delete fails
    let sends =
        || transport.sent().iter().flat_map(event_names).filter(|n| n == "custom.once").count();
    assert_eq!(sends(), 1, "precondition: accepted once");
    bind_fails.store(false, SeqCst);
    telemetry.flush().await; // the delete fails again; binding could now succeed
    telemetry.flush().await;
    assert_eq!(sends(), 1, "never sent again this launch");
    delete_fails.store(false, SeqCst);
    telemetry.flush().await;
    assert!(cache.pending().is_empty(), "deleted once the storage allows");
    assert_eq!(sends(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
