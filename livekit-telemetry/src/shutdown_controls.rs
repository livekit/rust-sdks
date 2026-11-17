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

//! Records that a retained public scope captures while the shutdown drain encodes.
//!
//! Each scenario runs the real exporter task and produces from inside a cache write, through a
//! weak scope, so the cache never keeps the pipeline alive. Before anything is asserted, the hook
//! is cleared, the task is joined (or aborted, with its real `JoinError` kept), and every owner is
//! dropped and observed destroyed. Delivery is read from processed acknowledgements (the
//! pipeline's `uploads_sent` equals what the transport accepted) or from a cache reopened after
//! the pipeline is gone: never from send attempts, never from a cache the pipeline still holds.
//! A continuous producer stops by itself after a few export timeouts, so a drain that never
//! yields still ends and the test fails rather than hangs. The real-time cases leave seconds of
//! margin for a loaded machine; their time limits are local progress checks, not a bound on how
//! long a drain or another pipeline's write can take (a started turn finishes its synchronous
//! writes, and every pipeline's writes pass one process-wide gate). The late-start rule is the
//! B1 case: its branch selection is forced, but it runs in real time, needs its setup's disk
//! writes to finish before a 2 s deadline, and retries a setup that did not.

use std::{
    future::Future,
    io::Read,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex, Weak,
    },
    task::{Context, Waker},
};

use opentelemetry_proto::tonic::{
    collector::{logs::v1::ExportLogsServiceRequest, trace::v1::ExportTraceServiceRequest},
    common::v1::any_value::Value,
};
use prost::Message;
use tokio::task::{JoinError, JoinHandle};

use super::*;
use crate::{
    BatchCache, FileCache, LogRecord, LogSource, MemoryCache, Scope, Severity, SpanName,
    SpanOutcome, Telemetry, TelemetryConfig, TelemetryEvent, TelemetryStats,
};

/// Runs inside a cache write; returns whether to stay armed.
type Hook = Box<dyn FnMut() -> bool + Send>;

/// Every pipeline's cache write passes one process-wide gate (`SpillCache`). The cases that
/// write in a loop, or block inside a write, would hold it again and again and starve each other
/// when the suite runs in parallel, so they run one at a time.
static GATE_HEAVY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A cache that counts its writes and runs a test hook inside each one.
struct HookedCache {
    inner: Box<dyn BatchCache>,
    pushes: AtomicUsize,
    hook: Mutex<Option<Hook>>,
}

impl HookedCache {
    fn new(inner: impl BatchCache + 'static) -> Arc<Self> {
        Arc::new(Self {
            inner: Box::new(inner),
            pushes: Default::default(),
            hook: Mutex::default(),
        })
    }

    fn arm(&self, hook: Hook) {
        *self.hook.lock().expect("lock") = Some(hook);
    }
}

impl BatchCache for HookedCache {
    fn push(&self, id: &str, body: &[u8]) -> std::io::Result<Vec<String>> {
        self.pushes.fetch_add(1, Ordering::SeqCst);
        // Taken out of the lock while it runs: a panicking hook poisons nothing.
        let hook = self.hook.lock().expect("lock").take();
        if let Some(mut hook) = hook {
            if hook() {
                self.hook.lock().expect("lock").get_or_insert(hook);
            }
        }
        self.inner.push(id, body)
    }
    fn replace(&self, old: &str, new: &[(String, Vec<u8>)]) -> std::io::Result<Vec<String>> {
        self.inner.replace(old, new)
    }
    fn pending(&self) -> Vec<String> {
        self.inner.pending()
    }
    fn read(&self, id: &str) -> std::io::Result<Vec<u8>> {
        self.inner.read(id)
    }
    fn remove(&self, id: &str) -> std::io::Result<()> {
        self.inner.remove(id)
    }
    fn clear(&self) -> std::io::Result<()> {
        self.inner.clear()
    }
}

/// A collector that acknowledges every request and keeps its signal and body.
#[derive(Default)]
struct Acks(Mutex<Vec<(bool, Vec<u8>)>>);

impl Acks {
    fn len(&self) -> usize {
        self.0.lock().expect("lock").len()
    }
}

#[async_trait::async_trait]
impl TelemetryTransport for Acks {
    async fn send(&self, request: ExportRequest) -> Result<crate::ExportResponse, ExportError> {
        let traces = request.url.contains("traces");
        self.0.lock().expect("lock").push((traces, request.body));
        Ok(crate::ExportResponse::accepted())
    }
}

/// Where delivery is observed.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    /// Online with a memory cache: acknowledged requests.
    MemoryAck,
    /// Online with a file cache: acknowledged requests, and the reopened directory is empty.
    FileAck,
    /// Offline with a file cache: nothing is sent; the batches are read from a reopened cache.
    FileReopen,
}

impl Mode {
    fn online(self) -> bool {
        self != Mode::FileReopen
    }
}

/// One pipeline with its exporter task, and weak references to observe its destruction.
struct Rig {
    telemetry: Telemetry,
    scope: Arc<Scope>,
    cache: Arc<HookedCache>,
    acks: Arc<Acks>,
    task: JoinHandle<()>,
    dir: Option<PathBuf>,
}

/// What a scenario observed, all owned: nothing in here keeps the pipeline alive.
#[derive(Debug)]
struct Verdict {
    joined: Result<(), JoinError>,
    destroyed: bool,
    stats: TelemetryStats,
    acked: usize,
    on_disk: usize,
    logs: Vec<String>,
    spans: Vec<String>,
}

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lk-shutdown-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

impl Rig {
    /// A started pipeline whose first tick is over, with one Room scope (online: routed to a
    /// granted project).
    async fn start(mode: Mode, name: &str, config: TelemetryConfig) -> Self {
        let dir = (mode != Mode::MemoryAck).then(|| dir(name));
        let cache = match &dir {
            Some(dir) => HookedCache::new(FileCache::open(dir, 1 << 24).expect("cache")),
            None => HookedCache::new(MemoryCache::new(1 << 24)),
        };
        let acks = Arc::new(Acks::default());
        let (telemetry, exporter) = Telemetry::with_cache(config, acks.clone(), cache.clone());
        let task = tokio::spawn(exporter.run());
        telemetry.flush().await;
        let scope = Arc::new(telemetry.begin_scope());
        if mode.online() {
            scope.set_server("wss://probe.livekit.cloud", &crate::destination::tests::granted(600));
            telemetry.flush().await;
        }
        Rig { telemetry, scope, cache, acks, task, dir }
    }

    /// Clear the hook, stop the task (abort it after `bound`), drop every owner and observe
    /// that each is gone; then read what was delivered.
    async fn finish(self, bound: Duration) -> Verdict {
        let Rig { telemetry, scope, cache, acks, task, dir } = self;
        let joined = join(task, bound).await;
        cache.hook.lock().expect("lock").take();
        let stats = telemetry.stats();
        let mut bodies = std::mem::take(&mut *acks.0.lock().expect("lock"));
        let acked = bodies.len();
        let weak = (
            Arc::downgrade(&telemetry.shared),
            Arc::downgrade(&scope),
            Arc::downgrade(&cache),
            Arc::downgrade(&acks),
        );
        drop((telemetry, scope, cache, acks));
        let destroyed = weak.0.upgrade().is_none()
            && weak.1.upgrade().is_none()
            && weak.2.upgrade().is_none()
            && weak.3.upgrade().is_none();
        let mut on_disk = 0;
        if let Some(dir) = dir {
            let reopened = FileCache::open(&dir, 1 << 24).expect("reopen");
            for id in reopened.pending() {
                let traces = matches!(BatchId::parse(&id).signal, Signal::Traces);
                bodies.push((traces, reopened.read(&id).expect("read")));
                on_disk += 1;
            }
            drop(reopened);
            let _ = std::fs::remove_dir_all(&dir);
        }
        let (logs, spans) = names(&bodies);
        Verdict { joined, destroyed, stats, acked, on_disk, logs, spans }
    }
}

/// Join the task; past `bound`, abort it and keep the real `JoinError`.
async fn join(mut task: JoinHandle<()>, bound: Duration) -> Result<(), JoinError> {
    match timeout(bound, &mut task).await {
        Ok(joined) => joined,
        Err(_) => {
            task.abort();
            task.await
        }
    }
}

/// The `app.*` names in gzipped OTLP bodies: log event names and bodies, then span names.
fn names(bodies: &[(bool, Vec<u8>)]) -> (Vec<String>, Vec<String>) {
    let (mut logs, mut spans) = (Vec::new(), Vec::new());
    for (traces, body) in bodies {
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(&body[..]).read_to_end(&mut raw).expect("gzip");
        if *traces {
            let request = ExportTraceServiceRequest::decode(&raw[..]).expect("traces");
            for span in request.resource_spans.into_iter().flat_map(|r| r.scope_spans) {
                spans.extend(span.spans.into_iter().map(|s| s.name));
            }
        } else {
            let request = ExportLogsServiceRequest::decode(&raw[..]).expect("logs");
            for record in request.resource_logs.into_iter().flat_map(|r| r.scope_logs) {
                for record in record.log_records {
                    let body = match record.body.and_then(|b| b.value) {
                        Some(Value::StringValue(body)) => body,
                        _ => String::new(),
                    };
                    logs.push(if record.event_name.is_empty() { body } else { record.event_name });
                }
            }
        }
    }
    logs.retain(|name| name.starts_with("app."));
    spans.retain(|name| name.starts_with("app."));
    logs.sort();
    spans.sort();
    (logs, spans)
}

/// A config with no tick, no threshold flush and no flood guard: only the test encodes.
fn quiet(max_queue_size: u32) -> TelemetryConfig {
    TelemetryConfig {
        max_events_per_10min: 0,
        flush_interval_ms: 3_600_000,
        stats_window_ms: 3_600_000,
        flush_threshold_bytes: u64::MAX,
        max_queue_size,
        ..TelemetryConfig::default()
    }
}

fn span(scope: &Scope, name: &str) {
    scope.start(SpanName::Custom { name: name.to_owned() }, None).end(SpanOutcome::Ok, None);
}

fn warn(scope: &Scope, body: &str) {
    scope.log(LogRecord {
        severity: Severity::Warn,
        source: LogSource::Ffi,
        body: body.to_owned(),
        logger: None,
        function: None,
        file: None,
        line: None,
        timestamp_ns: None,
        span_id: None,
    });
}

/// A hook that, on the first write it sees, captures through the weak scope from another
/// thread (joined before the write goes on), then disarms.
fn once(scope: Weak<Scope>, capture: impl FnOnce(&Scope) + Send + 'static) -> Hook {
    let mut capture = Some(capture);
    Box::new(move || {
        let (Some(scope), Some(capture)) = (scope.upgrade(), capture.take()) else { return false };
        std::thread::scope(|threads| {
            threads.spawn(|| capture(&scope));
        });
        false
    })
}

/// A record, a warning and a span captured by a retained scope while the final drain writes
/// its first batch reach the collector or the disk. The first write is the log batch when no
/// span is queued, the span batch otherwise. Before the fix, a shutdown encoded only what was
/// queued when its single turn started, and these were neither sent nor cached.
#[tokio::test(start_paused = true)]
async fn records_a_scope_captures_while_shutdown_encodes_are_delivered() {
    for mode in [Mode::MemoryAck, Mode::FileAck, Mode::FileReopen] {
        for span_first in [false, true] {
            let name = format!("deliver-{mode:?}-{span_first}");
            let rig = Rig::start(mode, &name, quiet(100)).await;
            if span_first {
                span(&rig.scope, "app.initial");
            }
            rig.scope.emit(TelemetryEvent::new("app.initial"));
            rig.cache.arm(once(Arc::downgrade(&rig.scope), |scope| {
                scope.emit(TelemetryEvent::new("app.late"));
                warn(scope, "app.late.warning");
                span(scope, "app.late");
            }));
            rig.telemetry.shutdown().await;
            let verdict = rig.finish(Duration::from_secs(5)).await;
            eprintln!("{name}: {verdict:?}");
            assert!(verdict.joined.is_ok() && verdict.destroyed, "{name}: cleanup first");
            assert_eq!(verdict.stats.uploads_sent as usize, verdict.acked, "{name}: acknowledged");
            if mode.online() {
                assert_eq!(verdict.on_disk, 0, "{name}: nothing left on disk");
            } else {
                assert_eq!(verdict.acked, 0, "{name}: offline sends nothing");
            }
            assert_eq!(verdict.stats.dropped, 0, "{name}: nothing dropped");
            assert_eq!(verdict.logs, ["app.initial", "app.late", "app.late.warning"], "{name}");
            let spans: &[&str] =
                if span_first { &["app.initial", "app.late"] } else { &["app.late"] };
            assert_eq!(verdict.spans, spans, "{name}");
        }
    }
}

/// Under queue pressure the queues still drop their oldest entries for new ones, counted as
/// `queue_full`, and the survivors are delivered. Two records and two spans are queued (the
/// queue holds two of each); the first write, of the two spans, lets five records and five spans
/// arrive. Eight are evicted (the two queued records and three of each kind of new arrival). The
/// two initial spans, the newest two records and the newest two spans survive, and every
/// survivor reaches the disk: no loss beyond the counted evictions.
#[tokio::test(start_paused = true)]
async fn queue_evictions_during_shutdown_are_counted_apart_from_delivery() {
    let rig = Rig::start(Mode::FileReopen, "pressure", quiet(2)).await;
    for name in ["app.initial0", "app.initial1"] {
        span(&rig.scope, name);
        rig.scope.emit(TelemetryEvent::new(name));
    }
    rig.cache.arm(once(Arc::downgrade(&rig.scope), |scope| {
        for i in 0..5 {
            scope.emit(TelemetryEvent::new(format!("app.late{i}")));
            span(scope, &format!("app.late{i}"));
        }
    }));
    rig.telemetry.shutdown().await;
    let verdict = rig.finish(Duration::from_secs(5)).await;
    eprintln!("pressure: {verdict:?}");
    assert!(verdict.joined.is_ok() && verdict.destroyed, "cleanup first");
    let stats = &verdict.stats;
    assert_eq!((stats.dropped_queue_full, stats.dropped), (8, 8), "evictions, and nothing else");
    assert_eq!(verdict.logs, ["app.late3", "app.late4"]);
    assert_eq!(verdict.spans, ["app.initial0", "app.initial1", "app.late3", "app.late4"]);
}

/// A continuous producer — one record and one span for every batch written — keeps the drain
/// encoding in turns until the original `export_timeout_ms` deadline, and no longer. The drain
/// neither stops while work keeps arriving and time is left, nor takes a new deadline per turn.
/// (Real time on two workers: a drain that never yields cannot hold back the test's clock.
/// The deadline is 3 s and the drain must stop within 3 s after it: margins for a loaded
/// machine, which can leave a worker idle for a second or more. That limit is a local progress
/// check, not a product bound: a started turn can take longer. The producer runs for five
/// deadlines, so a drain that takes a new deadline per turn, or loops until empty, is still
/// caught. If the drain does not stop by itself, the producer is stopped so the task can be
/// joined.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_continuous_producer_is_encoded_in_turns_until_the_original_deadline() {
    let _one_at_a_time = GATE_HEAVY.lock().await;
    let rig = Rig::start(
        Mode::MemoryAck,
        "continuous",
        TelemetryConfig { export_timeout_ms: 3000, ..quiet(1000) },
    )
    .await;
    let deadline = rig.telemetry.shared.config.export_timeout();
    let stop = Arc::new(AtomicBool::new(false));
    let (weak, stopped) = (Arc::downgrade(&rig.scope), stop.clone());
    let produced = Arc::new(AtomicUsize::new(0));
    let counted = produced.clone();
    let cap = std::time::Instant::now() + deadline * 5;
    rig.cache.arm(Box::new(move || {
        let live = !stopped.load(Ordering::SeqCst) && std::time::Instant::now() < cap;
        let Some(scope) = weak.upgrade().filter(|_| live) else { return false };
        let i = counted.fetch_add(1, Ordering::SeqCst);
        scope.emit(TelemetryEvent::new(format!("app.late{i}")));
        span(&scope, &format!("app.late{i}"));
        true
    }));
    rig.scope.emit(TelemetryEvent::new("app.initial"));
    let started = std::time::Instant::now();
    rig.telemetry.shutdown().await;
    let mut rig = rig;
    let stopped_itself = timeout(deadline * 5, &mut rig.task).await;
    let elapsed = started.elapsed();
    stop.store(true, Ordering::SeqCst);
    let stopped_itself = match stopped_itself {
        Ok(joined) => {
            rig.task = tokio::spawn(async {});
            Some(joined.is_ok())
        }
        Err(_) => None,
    };
    let produced = produced.load(Ordering::SeqCst);
    let verdict = rig.finish(Duration::from_secs(5)).await;
    eprintln!(
        "continuous: stopped_itself={stopped_itself:?} elapsed={elapsed:?} produced={produced} \
         acked={} late_logs={} late_spans={} joined={:?} destroyed={}",
        verdict.acked,
        verdict.logs.len(),
        verdict.spans.len(),
        verdict.joined,
        verdict.destroyed
    );
    assert!(verdict.joined.is_ok() && verdict.destroyed, "cleanup first");
    assert_eq!(stopped_itself, Some(true), "the drain stops by itself");
    assert!(elapsed >= deadline, "not before the deadline while work arrives: {elapsed:?}");
    assert!(elapsed < deadline * 2, "the original deadline: {elapsed:?}");
    assert_eq!(verdict.stats.uploads_sent as usize, verdict.acked, "acknowledged");
    // Many turns ran: each write produced one record, and well over one turn's worth arrived.
    assert!(produced > 10, "turns: {produced}");
    assert!(verdict.logs.len() > 10 && verdict.spans.len() > 10, "later turns were delivered");
}

/// An opt-out during shutdown turns stops them at once: the producer keeps capturing, but the
/// purge succeeds well before the deadline, nothing is sent after it returns, and nothing stays
/// on disk.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_opt_out_stops_shutdown_turns_without_waiting_for_the_deadline() {
    let _one_at_a_time = GATE_HEAVY.lock().await;
    let mut rig = Rig::start(
        Mode::FileAck,
        "purge",
        TelemetryConfig { export_timeout_ms: 20_000, ..quiet(1000) },
    )
    .await;
    let deadline = rig.telemetry.shared.config.export_timeout();
    let weak = Arc::downgrade(&rig.scope);
    let cap = std::time::Instant::now() + deadline * 3;
    rig.cache.arm(Box::new(move || {
        let Some(scope) = weak.upgrade().filter(|_| std::time::Instant::now() < cap) else {
            return false;
        };
        scope.emit(TelemetryEvent::new("app.late"));
        span(&scope, "app.late");
        true
    }));
    rig.scope.emit(TelemetryEvent::new("app.initial"));
    let started = std::time::Instant::now();
    let shutdown = {
        let telemetry = rig.telemetry.clone();
        tokio::spawn(async move { telemetry.shutdown().await })
    };
    // Wait for a few turns of the drain, within half the deadline, then opt out. (Each write
    // syncs the directory: slow when the whole suite runs at once.)
    let turns = timeout(deadline / 2, async {
        while rig.cache.pushes.load(Ordering::SeqCst) < 4 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .is_ok();
    let purged = rig.telemetry.purge().await;
    let elapsed = started.elapsed();
    let sent_at_purge = rig.acks.len();
    let shutdown = join(shutdown, Duration::from_secs(5)).await;
    let joined_task = timeout(Duration::from_secs(5), &mut rig.task).await;
    let stopped = matches!(joined_task, Ok(Ok(())));
    if joined_task.is_ok() {
        rig.task = tokio::spawn(async {});
    }
    let sent_later = rig.acks.len() - sent_at_purge;
    let verdict = rig.finish(Duration::from_secs(5)).await;
    eprintln!(
        "purge: turns={turns} purged={purged} elapsed={elapsed:?} sent_later={sent_later} \
         stopped={stopped} shutdown={shutdown:?} on_disk={} destroyed={}",
        verdict.on_disk, verdict.destroyed
    );
    assert!(verdict.joined.is_ok() && shutdown.is_ok() && verdict.destroyed, "cleanup first");
    assert!(turns, "the drain was running turns when the opt-out came");
    assert!(stopped, "the exporter stopped");
    assert!(purged, "the purge deleted everything");
    assert!(elapsed < deadline, "stopped by the opt-out, not the deadline: {elapsed:?}");
    assert_eq!(sent_later, 0, "nothing is sent after the purge returns");
    assert_eq!(verdict.on_disk, 0, "nothing stays on disk");
}

/// Each shutdown turn starts after a trip back to the executor, so on a single thread other
/// tasks run between turns and aborting the exporter takes effect at once, not at the deadline.
#[tokio::test(flavor = "current_thread")]
async fn shutdown_turns_leave_the_thread_to_other_tasks_and_can_be_aborted() {
    let _one_at_a_time = GATE_HEAVY.lock().await;
    // A long deadline: a loaded machine can leave this thread idle for a second.
    let config = TelemetryConfig { export_timeout_ms: 10_000, ..quiet(1000) };
    let rig = Rig::start(Mode::MemoryAck, "abort-turns", config).await;
    let deadline = rig.telemetry.shared.config.export_timeout();
    let weak = Arc::downgrade(&rig.scope);
    let cap = std::time::Instant::now() + deadline * 3;
    rig.cache.arm(Box::new(move || {
        let Some(scope) = weak.upgrade().filter(|_| std::time::Instant::now() < cap) else {
            return false;
        };
        scope.emit(TelemetryEvent::new("app.late"));
        true
    }));
    rig.scope.emit(TelemetryEvent::new("app.initial"));
    let started = std::time::Instant::now();
    let shutdown = {
        let telemetry = rig.telemetry.clone();
        tokio::spawn(async move { telemetry.shutdown().await })
    };
    // This task only runs again if the drain lets it.
    while rig.cache.pushes.load(Ordering::SeqCst) < 6 && started.elapsed() < deadline * 2 {
        tokio::task::yield_now().await;
    }
    let turns = rig.cache.pushes.load(Ordering::SeqCst);
    let elapsed = started.elapsed();
    let verdict = rig.finish(Duration::ZERO).await;
    let shutdown = join(shutdown, Duration::from_secs(5)).await;
    eprintln!("abort: turns={turns} elapsed={elapsed:?} joined={:?}", verdict.joined);
    assert!(shutdown.is_ok() && verdict.destroyed, "cleanup first");
    assert!(elapsed < deadline, "other tasks ran between turns: {elapsed:?}");
    assert!(verdict.joined.expect_err("aborted mid-drain").is_cancelled());
}

/// What one late resumption of a suspended shutdown drain did.
#[derive(Debug)]
struct Resumed {
    suspended: bool,
    completed: bool,
    writes_before: usize,
    writes_after: usize,
    destroyed: bool,
    logs: Vec<String>,
    spans: Vec<String>,
}

/// Suspend the drain at the yield before its second turn, with a record and a span that
/// arrived during the first turn's write queued; let `after` of real time pass; then resume it
/// once. The exporter lives in a runtime that is entered but never driven, so its timers never
/// fire: once the deadline has passed by the clock, the yielded turn is still the only ready
/// branch, whatever `select!` would pick. (That is a legal schedule: an executor can poll a task
/// before it processes the timers that expired meanwhile.) The test owns and polls the exporter
/// future itself, so no task or thread outlives the poll, and everything is dropped and observed
/// destroyed before the cache is reopened.
fn resume_once(name: &str, after: Duration) -> Resumed {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_time().build().expect("rt");
    let entered = runtime.enter();
    let dir = dir(name);
    let cache = HookedCache::new(FileCache::open(&dir, 1 << 24).expect("cache"));
    let acks = Arc::new(Acks::default());
    let config = TelemetryConfig { export_timeout_ms: 2000, ..quiet(100) };
    let (telemetry, exporter) = Telemetry::with_cache(config, acks.clone(), cache.clone());
    let noop = &mut Context::from_waker(Waker::noop());
    let mut run = Box::pin(exporter.run());
    // A flush the exporter answers settles its start before anything is queued.
    let mut flush = Box::pin(telemetry.flush());
    let _ = flush.as_mut().poll(noop);
    let _ = run.as_mut().poll(noop);
    let settled = flush.as_mut().poll(noop).is_ready();
    drop(flush);
    let scope = Arc::new(telemetry.begin_scope());
    scope.emit(TelemetryEvent::new("app.initial"));
    cache.arm(once(Arc::downgrade(&scope), |scope| {
        scope.emit(TelemetryEvent::new("app.late"));
        span(scope, "app.late");
    }));
    let mut shutdown = Box::pin(telemetry.shutdown());
    let _ = shutdown.as_mut().poll(noop);
    // Polled even when the flush did not settle, so a completed drain is never polled again.
    let first = run.as_mut().poll(noop);
    let suspended = settled && first.is_pending();
    let writes_before = cache.pushes.load(Ordering::SeqCst);
    std::thread::sleep(after);
    let completed = first.is_ready() || run.as_mut().poll(noop).is_ready();
    let writes_after = cache.pushes.load(Ordering::SeqCst);
    let _ = shutdown.as_mut().poll(noop);
    drop((shutdown, run));
    cache.hook.lock().expect("lock").take();
    let weak = (
        Arc::downgrade(&telemetry.shared),
        Arc::downgrade(&scope),
        Arc::downgrade(&cache),
        Arc::downgrade(&acks),
    );
    drop((telemetry, scope, cache, acks));
    let destroyed = weak.0.upgrade().is_none()
        && weak.1.upgrade().is_none()
        && weak.2.upgrade().is_none()
        && weak.3.upgrade().is_none();
    drop(entered);
    drop(runtime);
    let reopened = FileCache::open(&dir, 1 << 24).expect("reopen");
    let bodies: Vec<_> = reopened
        .pending()
        .into_iter()
        .map(|id| {
            let traces = matches!(BatchId::parse(&id).signal, Signal::Traces);
            (traces, reopened.read(&id).expect("read"))
        })
        .collect();
    drop(reopened);
    let _ = std::fs::remove_dir_all(&dir);
    let (logs, spans) = names(&bodies);
    Resumed { suspended, completed, writes_before, writes_after, destroyed, logs, spans }
}

/// [`resume_once`], counted only when the first turn ended before the deadline: its two
/// synchronous file writes can take longer on a cold disk, and then the drain rightly ends
/// without a second turn. Up to three attempts. A discarded attempt also drops its owners and
/// computes `destroyed`, but only the returned attempt's cleanup is asserted.
fn resume(name: &str, after: Duration) -> Resumed {
    let mut attempt = 0;
    loop {
        let resumed = resume_once(&format!("{name}-{attempt}"), after);
        attempt += 1;
        if resumed.suspended || attempt == 3 {
            return resumed;
        }
    }
}

/// A turn never starts after the original deadline, even when the drain is resumed late with
/// its yielded turn ready and the deadline branch not. The control resumes the same drain
/// before the deadline: the same branch then runs the turn and the late record and span reach
/// the disk, so the first case fails for the right reason. The first turn writes two batches:
/// the scope's record, and the shutdown self-report, which belongs to the process.
#[test]
fn a_drain_resumed_after_its_deadline_starts_no_turn() {
    let late = resume("resume-late", Duration::from_millis(2300));
    let early = resume("resume-early", Duration::ZERO);
    eprintln!("late: {late:?}\nearly: {early:?}");
    for verdict in [&late, &early] {
        assert!(verdict.destroyed, "cleanup first");
        assert!(verdict.suspended && verdict.completed, "{verdict:?}");
        assert_eq!(verdict.writes_before, 2, "the first turn's two batches");
    }
    assert_eq!(late.writes_after, late.writes_before, "no turn after the deadline");
    assert_eq!(late.logs, ["app.initial"]);
    assert!(late.spans.is_empty());
    assert_eq!(early.writes_after, early.writes_before + 2, "a turn before the deadline");
    assert_eq!(early.logs, ["app.initial", "app.late"]);
    assert_eq!(early.spans, ["app.late"]);
}

/// A turn that has started may finish its writes after the deadline: the cache is synchronous.
/// Here the first turn's first write takes past the deadline (real time, a blocking write). The
/// turn completes, its two batches (the scope's record and the process's self-report) stay on
/// disk, but the record and span that arrived during the crossing write get no further turn.
#[tokio::test(flavor = "current_thread")]
async fn a_write_that_crosses_the_deadline_finishes_but_no_turn_follows() {
    let _one_at_a_time = GATE_HEAVY.lock().await;
    let config = TelemetryConfig { export_timeout_ms: 1000, ..quiet(100) };
    let rig = Rig::start(Mode::FileReopen, "crossing", config).await;
    let deadline = rig.telemetry.shared.config.export_timeout();
    rig.scope.emit(TelemetryEvent::new("app.initial"));
    rig.cache.arm(once(Arc::downgrade(&rig.scope), move |scope| {
        scope.emit(TelemetryEvent::new("app.late"));
        span(scope, "app.late");
        std::thread::sleep(deadline + Duration::from_millis(200));
    }));
    rig.telemetry.shutdown().await;
    let pushes = rig.cache.pushes.load(Ordering::SeqCst);
    let verdict = rig.finish(Duration::from_secs(5)).await;
    eprintln!("crossing: pushes={pushes} logs={:?} spans={:?}", verdict.logs, verdict.spans);
    assert!(verdict.joined.is_ok() && verdict.destroyed, "cleanup first");
    assert_eq!(pushes, 2, "the first turn's two batches, and no turn after them");
    assert_eq!(verdict.logs, ["app.initial"], "the crossing write completed");
    assert!(verdict.spans.is_empty());
}

/// The harness itself: a panicking exporter surfaces as its real `JoinError`, and the pipeline
/// is still freed before the verdict.
#[tokio::test(start_paused = true)]
async fn a_panicking_exporter_is_reported_and_the_pipeline_still_freed() {
    let rig = Rig::start(Mode::FileReopen, "panic", quiet(100)).await;
    rig.scope.emit(TelemetryEvent::new("app.initial"));
    rig.cache.arm(Box::new(|| panic!("forced failure inside a cache write")));
    rig.telemetry.shutdown().await;
    let verdict = rig.finish(Duration::from_secs(5)).await;
    assert!(verdict.destroyed, "freed despite the panic");
    assert!(verdict.joined.expect_err("the task panicked").is_panic());
}

/// The harness itself: an exporter that does not stop in time is aborted, surfaces as a
/// cancelled `JoinError`, and the pipeline is still freed.
#[tokio::test(start_paused = true)]
async fn an_exporter_that_does_not_stop_is_aborted_and_the_pipeline_still_freed() {
    let rig = Rig::start(Mode::MemoryAck, "abort", quiet(100)).await;
    let verdict = rig.finish(Duration::from_millis(10)).await;
    assert!(verdict.destroyed, "freed after the abort");
    assert!(verdict.joined.expect_err("the task was aborted").is_cancelled());
}
