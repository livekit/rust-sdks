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

//! Claim-specific actor probes; bounded observations never wake the exporter.

use std::{
    future::poll_fn,
    sync::{atomic::Ordering, Weak},
};

use prost::Message;
use tokio::task::JoinHandle;

use super::*;
use crate::{SpanName, SpanOutcome, Telemetry, TelemetryConfig};

/// A complete request and the actual instant the exporter called its transport.
type Sent = (Instant, ExportRequest);

/// A transport with a bounded test-owned output queue and no owner back-reference.
struct ObservedTransport(mpsc::Sender<Sent>);

#[async_trait::async_trait]
impl TelemetryTransport for ObservedTransport {
    async fn send(&self, request: ExportRequest) -> Result<crate::ExportResponse, ExportError> {
        self.0.try_send((Instant::now(), request)).map_err(|_| ExportError::Retryable {
            reason: "test observation queue full or closed".into(),
            retry_after_ms: None,
        })?;
        Ok(crate::ExportResponse::accepted())
    }
}

/// An observation after the real actor returned Pending to its executor.
#[derive(Debug)]
struct Idle {
    at: Instant,
    release_before_poll: bool,
    release_remaining: bool,
    offline: bool,
    sensitive: bool,
    cached: usize,
}

/// Construct the normal public pipeline with a transport that only records requests.
fn pipeline(
    config: TelemetryConfig,
) -> (Telemetry, Exporter, Arc<ObservedTransport>, mpsc::Receiver<Sent>) {
    let (sent, received) = mpsc::channel(16);
    let transport = Arc::new(ObservedTransport(sent));
    let (telemetry, exporter) = Telemetry::new(config, transport.clone());
    (telemetry, exporter, transport, received)
}

/// Poll the real actor on a real owned Tokio task; observations only flow out of it.
fn observe(exporter: Exporter, shared: Weak<Shared>) -> (JoinHandle<()>, mpsc::Receiver<Idle>) {
    let (observed, idle) = mpsc::channel(32);
    let mut run = Box::pin(exporter.run());
    let task = tokio::spawn(poll_fn(move |cx| {
        let release_before_poll =
            shared.upgrade().is_some_and(|s| s.released.load(Ordering::SeqCst));
        let result = run.as_mut().poll(cx);
        if result.is_pending() {
            if let Some(shared) = shared.upgrade() {
                let offline = shared.device.lock().unwrap().unwrap_or_default().network
                    == NetworkType::Unavailable;
                let sensitive = shared.spans.lock().unwrap().any_open(SENSITIVE_SPANS);
                let _ = observed.try_send(Idle {
                    at: Instant::now(),
                    release_before_poll,
                    release_remaining: shared.released.load(Ordering::SeqCst),
                    offline,
                    sensitive,
                    cached: shared.cache.pending().len(),
                });
            }
        }
        result
    }));
    (task, idle)
}

/// Await an actual actor poll observation, with a deadline that excludes ordinary ticks.
async fn idle_matching(
    idle: &mut mpsc::Receiver<Idle>,
    matches: impl Fn(&Idle) -> bool,
) -> Option<Idle> {
    timeout(Duration::from_millis(1), async {
        while let Some(state) = idle.recv().await {
            if matches(&state) {
                return Some(state);
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}

/// Cancel/purge and join on success and failure before dropping the last pipeline owner.
async fn cleanup(telemetry: Telemetry, mut task: JoinHandle<()>) -> bool {
    let purged = timeout(Duration::from_secs(2), telemetry.purge()).await == Ok(true);
    let joined = match timeout(Duration::from_secs(2), &mut task).await {
        Ok(result) => result.is_ok(),
        Err(_) => {
            task.abort();
            let _ = task.await;
            false
        }
    };
    drop(telemetry);
    purged && joined
}

/// Ending the actual last typed sensitive span is the sole wake at t=3, with no flush or tick.
#[tokio::test(start_paused = true)]
async fn last_sensitive_span_automatically_wakes_cached_upload_at_three_seconds() {
    for last_connect in [false, true] {
        let (telemetry, exporter, transport, mut sends) = pipeline(TelemetryConfig::default());
        let shared = Arc::downgrade(&telemetry.shared);
        let cache = Arc::downgrade(&telemetry.shared.cache);
        let transport_weak = Arc::downgrade(&transport);
        let room = telemetry.begin_scope();
        room.set_server("wss://probe.livekit.cloud", &crate::destination::tests::granted(600));
        let reconnect = SpanName::Reconnect { reason: crate::ReconnectReason::SignalDisconnected };
        let (first_name, last_name) = if last_connect {
            (reconnect, SpanName::Connect)
        } else {
            (SpanName::Connect, reconnect)
        };
        let first = room.start(first_name, None);
        let last = room.start(last_name, None);
        room.emit_custom("already_waiting", Vec::new());
        let start = Instant::now();
        let (task, mut idle) = observe(exporter, shared.clone());
        let waiting =
            idle_matching(&mut idle, |s| s.sensitive && s.cached > 0 && !s.release_remaining).await;
        tokio::time::advance(Duration::from_secs(2)).await;
        first.end(SpanOutcome::Ok, None);
        let non_last_sent = timeout(Duration::from_millis(1), sends.recv()).await.ok().flatten();
        tokio::time::advance(Duration::from_millis(999)).await;
        let ended = Instant::now();
        last.end(SpanOutcome::Ok, None);
        let sent = timeout(Duration::from_millis(1), sends.recv()).await.ok().flatten();
        let resumed = idle_matching(&mut idle, |s| {
            s.at >= ended && s.release_before_poll && !s.release_remaining && !s.sensitive
        })
        .await;
        let observed = sent.as_ref().map(|(at, _)| *at);
        drop((first, last, room));
        let stopped = cleanup(telemetry, task).await;
        drop(transport);
        let destroyed = shared.upgrade().is_none()
            && cache.upgrade().is_none()
            && transport_weak.upgrade().is_none();
        eprintln!("V03 last_connect={last_connect} waiting={waiting:?} ended={:?} sent={:?} resumed={resumed:?} stopped={stopped} destroyed={destroyed}", ended-start, observed.map(|at| at-start));
        assert!(stopped && destroyed, "cleanup before verdict");
        assert!(waiting.is_some() && non_last_sent.is_none());
        assert_eq!(ended - start, Duration::from_secs(3));
        assert_eq!(observed, Some(ended), "the last span's release, not the 60s tick/cap");
        assert!(resumed.is_some(), "actual executor poll observed Shared::released");
    }
}

/// Public typed/report callers preserve layered byte growth and small-stream CPU pressure.
#[tokio::test(start_paused = true)]
async fn public_rtc_callers_keep_batch_growth_and_per_stream_cpu_pressure() {
    use crate::{AttributeValue, RtcStat, RtcStatsSample, StreamDirection, TrackKind};
    let (telemetry, exporter, transport, mut sends) =
        pipeline(TelemetryConfig { flush_interval_ms: 1000, ..Default::default() });
    let shared = Arc::downgrade(&telemetry.shared);
    let transport_weak = Arc::downgrade(&transport);
    let room = telemetry.begin_scope();
    room.set_server("wss://probe.livekit.cloud", &crate::destination::tests::granted(600));
    let initial_poll = room.stats_poll_interval_ms();
    for (poll, bytes) in [[1_000, 4_000], [5_000, 12_000]].into_iter().enumerate() {
        let timestamp = Some((poll as u64 + 1) * 1_000_000_000);
        let cpu = [100 + 100 * poll as u64, 1_000];
        let report = ["h", "f"]
            .into_iter()
            .enumerate()
            .map(|(index, rid)| RtcStat {
                kind: "outbound-rtp".into(),
                id: format!("OUT_{rid}"),
                members: HashMap::from([
                    ("rid".into(), AttributeValue::Str(rid.into())),
                    ("bytesSent".into(), AttributeValue::Int(bytes[index] as i64)),
                    ("qualityLimitationDurations.cpu".into(), (cpu[index] as f64 / 1000.0).into()),
                ]),
            })
            .collect();
        room.record_stats_report(
            "TR_report",
            TrackKind::Video,
            crate::StreamDirection::Outbound,
            report,
            timestamp,
        );
        for (index, layer) in ["h", "f"].into_iter().enumerate() {
            room.record_stats(RtcStatsSample {
                track_id: "TR_typed".into(),
                layer: Some(layer.into()),
                bytes: Some(bytes[index]),
                quality_limitation_cpu_ms: Some(cpu[index]),
                timestamp_ns: timestamp,
                ..RtcStatsSample::new("TR_typed", TrackKind::Video, StreamDirection::Outbound)
            });
        }
    }
    let pressure_poll = room.stats_poll_interval_ms();
    let (task, mut idle) = observe(exporter, shared.clone());
    let initial_idle = idle_matching(&mut idle, |s| !s.release_remaining).await;
    tokio::time::advance(Duration::from_secs(120)).await;
    let mut growth = std::collections::BTreeMap::new();
    let mut identity_counts = Vec::new();
    let wire_observed = timeout(Duration::from_millis(1), async {
        while let Some((_, request)) = sends.recv().await {
            let Some(body) = gunzip(&request.body) else { continue };
            let Ok(decoded) = crate::proto::opentelemetry::proto::collector::logs::v1::ExportLogsServiceRequest::decode(body.as_slice()) else { continue };
            for record in decoded.resource_logs.into_iter().flat_map(|r| r.scope_logs).flat_map(|s| s.log_records) {
                let identity: Vec<_> = record.attributes.iter().filter(|a| matches!(a.key.as_str(), "lk.track_id" | "lk.track.sid")).collect();
                let track = identity.first().and_then(|a| a.value.as_ref()).and_then(|v| v.value.as_ref()).and_then(|v| match v { crate::proto::opentelemetry::proto::common::v1::any_value::Value::StringValue(s) => Some(s.clone()), _ => None });
                let bytes = record.attributes.iter().find(|a| a.key == "lk.rtc.bytes").and_then(|a| a.value.as_ref()).and_then(|v| v.value.as_ref()).and_then(|v| match v { crate::proto::opentelemetry::proto::common::v1::any_value::Value::IntValue(n) => Some(*n), _ => None });
                if let (Some(track), Some(bytes)) = (track, bytes) {
                    identity_counts.push(identity.len());
                    growth.insert(track, bytes);
                }
            }
            if growth.len() == 2 { return true; }
        }
        false
    }).await == Ok(true);
    drop(room);
    let stopped = cleanup(telemetry, task).await;
    drop(transport);
    let destroyed = shared.upgrade().is_none() && transport_weak.upgrade().is_none();
    eprintln!("V07 poll_interval={initial_poll}->{pressure_poll} growth={growth:?} identity_counts={identity_counts:?} stopped={stopped} destroyed={destroyed}");
    assert!(stopped && destroyed, "cleanup before verdict");
    assert!(initial_idle.is_some() && wire_observed);
    assert_eq!((initial_poll, pressure_poll), (30_000, 60_000));
    assert_eq!(growth, [("TR_report".to_owned(), 12_000), ("TR_typed".to_owned(), 12_000)].into());
    assert_eq!(identity_counts, [1, 1], "one identity key per record, never dual emission");
}

/// Run the documented soft→offline→reconnect timeline without flushes or polling sleeps.
async fn offline_timeline(reconnect: u64) -> (bool, Option<Duration>, u64, bool) {
    let (telemetry, exporter, transport, mut sends) = pipeline(TelemetryConfig {
        flush_interval_ms: 600_000,
        stats_window_ms: 600_000,
        ..Default::default()
    });
    let shared = Arc::downgrade(&telemetry.shared);
    let transport_weak = Arc::downgrade(&transport);
    let room = telemetry.begin_scope();
    room.set_server("wss://probe.livekit.cloud", &crate::destination::tests::granted(600));
    let soft =
        DeviceState { network: NetworkType::Wifi, network_constrained: true, ..Default::default() };
    telemetry.set_device_state(soft);
    room.emit_custom("soft_hold_backlog", Vec::new());
    let start = Instant::now();
    let (task, mut idle) = observe(exporter, shared.clone());
    let initial =
        idle_matching(&mut idle, |s| s.cached > 0 && !s.offline && !s.release_remaining).await;
    tokio::time::advance(Duration::from_secs(5)).await;
    telemetry.set_device_state(DeviceState { network: NetworkType::Unavailable, ..soft });
    let offline =
        idle_matching(&mut idle, |s| s.offline && s.cached > 0 && !s.release_remaining).await;
    tokio::time::advance(Duration::from_secs(reconnect - 5)).await;
    let sent_offline = sends.try_recv().is_ok();
    telemetry.set_device_state(soft);
    let connected = idle_matching(&mut idle, |s| {
        !s.offline
            && s.at >= start + Duration::from_secs(reconnect)
            && s.release_before_poll
            && !s.release_remaining
    })
    .await;
    let early = if reconnect < 60 { sends.try_recv().ok() } else { None };
    if reconnect < 60 {
        tokio::time::advance(Duration::from_secs(60 - reconnect)).await;
    }
    let sent = timeout(Duration::from_millis(1), sends.recv()).await.ok().flatten();
    let elapsed = sent.as_ref().map(|(at, _)| *at - start);
    let capped = telemetry.stats().holds_capped;
    drop(room);
    let stopped = cleanup(telemetry, task).await;
    drop(transport);
    let destroyed = shared.upgrade().is_none() && transport_weak.upgrade().is_none();
    eprintln!("V04 reconnect={reconnect}s initial={initial:?} offline={offline:?} connected={connected:?} sent_offline={sent_offline} early={:?} sent={elapsed:?} cap_hits={capped} stopped={stopped} destroyed={destroyed}", early.as_ref().map(|(at,_)| *at-start));
    (
        initial.is_some()
            && offline.is_some()
            && connected.is_some()
            && !sent_offline
            && early.is_none(),
        elapsed,
        capped,
        stopped && destroyed,
    )
}

/// Offline forbids sends; elapsed offline time intentionally consumes the soft hold's cap.
#[tokio::test(start_paused = true)]
async fn offline_time_counts_toward_soft_hold_with_early_reconnect_control() {
    let early = offline_timeline(30).await;
    let late = offline_timeline(120).await;
    assert!(early.3 && late.3, "both pipelines joined and destroyed before verdict");
    assert!(early.0 && late.0, "no upload while offline or before the soft cap");
    assert_eq!((early.1, late.1), (Some(Duration::from_secs(60)), Some(Duration::from_secs(120))));
    assert_eq!((early.2, late.2), (1, 1));
}

/// Exactly 127/128 caller bytes are admitted, 129 refused, independently of UTF-8 character count.
#[tokio::test(start_paused = true)]
async fn caller_name_byte_boundary_and_prefix_are_preserved_on_the_wire() {
    let (telemetry, exporter, transport, mut sends) = pipeline(TelemetryConfig::default());
    let shared = Arc::downgrade(&telemetry.shared);
    let transport_weak = Arc::downgrade(&transport);
    let room = telemetry.begin_scope();
    room.set_server("wss://probe.livekit.cloud", &crate::destination::tests::granted(600));
    let names = [
        "x".repeat(127),
        "x".repeat(128),
        "x".repeat(129),
        format!("{}a", "é".repeat(63)),
        "é".repeat(64),
        format!("{}a", "é".repeat(64)),
    ];
    for name in &names {
        room.emit_custom(name, Vec::new());
    }
    let (task, _idle) = observe(exporter, shared.clone());
    let sent = timeout(Duration::from_millis(1), sends.recv()).await.ok().flatten();
    let decoded = sent.as_ref().and_then(|(_, request)| gunzip(&request.body)).and_then(|body| {
        crate::proto::opentelemetry::proto::collector::logs::v1::ExportLogsServiceRequest::decode(
            body.as_slice(),
        )
        .ok()
    });
    let mut actual: Vec<String> = decoded
        .into_iter()
        .flat_map(|r| r.resource_logs)
        .flat_map(|r| r.scope_logs)
        .flat_map(|s| s.log_records)
        .map(|r| r.event_name)
        .filter(|name| name.starts_with("custom."))
        .collect();
    actual.sort();
    let mut expected: Vec<_> = names
        .iter()
        .filter(|name| name.len() <= 128)
        .map(|name| format!("custom.{name}"))
        .collect();
    expected.sort();
    let stats = telemetry.stats();
    drop(room);
    let stopped = cleanup(telemetry, task).await;
    drop(transport);
    let destroyed = shared.upgrade().is_none() && transport_weak.upgrade().is_none();
    eprintln!("V05 caller_lengths={:?} wire_lengths={:?} invalid={} rate_limited={} stopped={stopped} destroyed={destroyed}", names.iter().map(String::len).collect::<Vec<_>>(), actual.iter().map(String::len).collect::<Vec<_>>(), stats.dropped_invalid, stats.dropped_rate_limited);
    assert!(stopped && destroyed, "cleanup before verdict");
    assert_eq!(actual, expected);
    assert_eq!(stats.dropped_invalid, 2);
    assert_eq!(stats.dropped_rate_limited, 0);
}

/// A cache that counts its writes and, while a batch is written, lets a test produce more
/// records. A hook captures its pipeline weakly, so the cache never keeps it alive.
struct CountingCache {
    inner: crate::MemoryCache,
    pushes: std::sync::atomic::AtomicUsize,
    during_push: std::sync::Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl CountingCache {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: crate::MemoryCache::new(1 << 20),
            pushes: Default::default(),
            during_push: Default::default(),
        })
    }
}

impl crate::BatchCache for CountingCache {
    fn push(&self, id: &str, body: &[u8]) -> std::io::Result<Vec<String>> {
        self.pushes.fetch_add(1, Ordering::SeqCst);
        if let Some(hook) = self.during_push.lock().expect("lock").as_ref() {
            hook();
        }
        crate::BatchCache::push(&self.inner, id, body)
    }
    fn replace(&self, old: &str, new: &[(String, Vec<u8>)]) -> std::io::Result<Vec<String>> {
        crate::BatchCache::replace(&self.inner, old, new)
    }
    fn pending(&self) -> Vec<String> {
        crate::BatchCache::pending(&self.inner)
    }
    fn read(&self, id: &str) -> std::io::Result<Vec<u8>> {
        crate::BatchCache::read(&self.inner, id)
    }
    fn remove(&self, id: &str) -> std::io::Result<()> {
        crate::BatchCache::remove(&self.inner, id)
    }
    fn clear(&self) -> std::io::Result<()> {
        crate::BatchCache::clear(&self.inner)
    }
}

/// One encoding turn is bounded by the counts taken when it starts: as many records and as many
/// finished spans as were queued then, both counted before anything is encoded. Here each batch
/// written lets a producer add one record and one finished span, three times. They are left for
/// later turns, which encode them; no turn grows with what arrives during it. (The queue is far
/// from full, so nothing is dropped; this test says nothing about a shutdown.)
#[tokio::test(start_paused = true)]
async fn an_encoding_turn_is_bounded_by_the_counts_at_its_start() {
    let cache = CountingCache::new();
    let config = TelemetryConfig { max_events_per_10min: 0, ..TelemetryConfig::default() };
    let transport = Arc::new(crate::exporter::tests::Collector::default());
    let (telemetry, mut exporter) = Telemetry::with_cache(config, transport, cache.clone());
    let (producer, late) = (Arc::new(telemetry.clone()), std::sync::atomic::AtomicUsize::new(0));
    let weak = Arc::downgrade(&producer);
    *cache.during_push.lock().expect("lock") = Some(Box::new(move || {
        let Some(producer) = weak.upgrade() else { return };
        if late.fetch_add(1, Ordering::SeqCst) < 3 {
            producer.emit(crate::TelemetryEvent::new("app.late"));
            producer.begin_scope().start(SpanName::Publish, None).end(SpanOutcome::Ok, None);
        }
    }));
    telemetry.emit(crate::TelemetryEvent::new("app.first"));
    // After each turn: batches written, then records and finished spans still queued.
    let mut turns = Vec::new();
    for _ in 0..4 {
        exporter.enqueue();
        let spans = telemetry.shared.spans.lock().expect("lock").finished_len();
        turns.push((cache.pushes.swap(0, Ordering::SeqCst), telemetry.shared.store.len(), spans));
    }
    // Torn down before the verdict: the hook, every handle, the exporter, then the pipeline.
    cache.during_push.lock().expect("lock").take();
    let shared = Arc::downgrade(&telemetry.shared);
    drop((producer, telemetry, exporter));
    assert!(shared.upgrade().is_none(), "the pipeline is released");
    // Turn 1 writes the one record queued at its start. Turn 2 writes the span and the record
    // that arrived during turn 1, and two more of each arrive. Turn 3 writes those: one batch
    // of records and one batch for each of the two spans' sessions. Turn 4 has nothing to write.
    assert_eq!(turns, [(1, 1, 1), (2, 2, 2), (3, 0, 0), (0, 0, 0)]);
}
