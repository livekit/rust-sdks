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

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    time::Duration,
};

use tokio::sync::{mpsc, oneshot};
use tokio::time::{timeout, Instant};

use crate::span::SpanKind;
use crate::{
    cache::SpillCache,
    destination::{Destinations, ENDPOINT_OVERRIDE_ENV},
    event::now_unix_nanos,
    exporter::Command,
    rtc::StatsWindows,
    scope::{Scope, ScopeState},
    span::Spans,
    stats::{Counters, TelemetryStatus},
    store::{Queued, Store},
    Attribute, AttributeValue, BatchCache, DeviceState, Exporter, FileCache, LogRecord, LogSource,
    MemoryCache, RtcStatsSample, Severity, SpanOutcome, TelemetryEvent, TelemetryStats,
    TelemetryTransport,
};
use crate::{DeviceEvent, Span, SpanName};

/// Pipeline configuration: storage and tuning. Where batches go is not configurable — the core
/// derives it from the server URL and token each room hands over ([`Scope::set_server`]).
///
/// Defaults are conservative, sized so a fleet cannot overload the collector: one export and
/// one RTC stats window per minute, OTel's `BatchLogRecordProcessor` queue (2048) and batch (512).
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Resource attributes describing the emitter (`service.name`, `os.name`,
    /// `device.model.identifier`, `session.id`, …). `telemetry.sdk.*` are filled in by the core.
    #[cfg_attr(feature = "uniffi", uniffi(default = []))]
    pub resource: Vec<Attribute>,
    /// Who is reporting, typed; the core owns the semconv keys. Extra attributes go in `resource`.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub sdk: Option<TelemetryResource>,
    /// Directory for the on-disk batch cache (created if missing; its parent must exist).
    /// `None` keeps batches in memory only: they survive failed uploads, not the process.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub storage_dir: Option<String>,
    /// Cap on cached batches, in memory or on disk; the oldest are evicted first.
    #[cfg_attr(feature = "uniffi", uniffi(default = 4194304))]
    pub max_cache_bytes: u64,
    /// Base export cadence; stretched up to 4× by [`DeviceState::cadence_factor`].
    #[cfg_attr(feature = "uniffi", uniffi(default = 60000))]
    pub flush_interval_ms: u64,
    /// Events buffered before the oldest are dropped.
    #[cfg_attr(feature = "uniffi", uniffi(default = 2048))]
    pub max_queue_size: u32,
    /// Events per export request.
    #[cfg_attr(feature = "uniffi", uniffi(default = 512))]
    pub max_batch_size: u32,
    /// Bound on a single transport attempt, and on `shutdown`.
    #[cfg_attr(feature = "uniffi", uniffi(default = 10000))]
    pub export_timeout_ms: u64,
    /// RTC stats window: readings pushed with [`Scope::record_stats`] are summarised into one
    /// `lk.rtc.stats.sample` per track and direction every window (stretched like the cadence).
    #[cfg_attr(feature = "uniffi", uniffi(default = 60000))]
    pub stats_window_ms: u64,
    /// Flood guard for discrete events: beyond this many `emit`s per 10 minutes the rest are
    /// dropped and counted as `rate_limited`. RTC windows and self-telemetry are exempt; 0 = off.
    #[cfg_attr(feature = "uniffi", uniffi(default = 300))]
    pub max_events_per_10min: u32,
    /// Cached batches uploaded per tick while a session may be live — bounds how fast a backlog
    /// (offline period, previous launch) replays next to a call: 4 × ~20 KB gzipped per minute
    /// is ~10 kbps. `shutdown` drains without the budget.
    #[cfg_attr(feature = "uniffi", uniffi(default = 4))]
    pub max_batches_per_upload: u32,
    /// Export as soon as the queue holds about this many bytes, without waiting for the tick
    /// (design doc: "flush on the tick or at 256 KB").
    #[cfg_attr(feature = "uniffi", uniffi(default = 262144))]
    pub flush_threshold_bytes: u64,
    /// Cap on one request's payload before compression (design doc: "single POST ≤ 1 MB").
    #[cfg_attr(feature = "uniffi", uniffi(default = 1048576))]
    pub max_batch_bytes: u64,
    /// Lowest severity a plain log record (an event with no name) needs to leave the device.
    /// Events are not subject to it. Design doc: warn.
    /// `None` is `Warn`. (Optional because UniFFI 0.31 cannot default an enum literal.)
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub log_severity: Option<Severity>,
    /// Platform instruments not to run; all run by default.
    #[cfg_attr(feature = "uniffi", uniffi(default = []))]
    pub disabled_instruments: Vec<Instrument>,
}

/// The platform instruments a config can switch off.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instrument {
    /// Thermal, power, memory, network, battery, audio session.
    Device,
    /// Warn/error lines from the SDK, the core and WebRTC.
    Logs,
    /// `getStats` windows per track and subscribe spans.
    Rtc,
    /// Connect, reconnect and publish spans.
    Room,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            resource: Vec::new(),
            sdk: None,
            storage_dir: None,
            max_cache_bytes: 4 * 1024 * 1024,
            flush_interval_ms: 60_000,
            max_queue_size: 2048,
            max_batch_size: 512,
            export_timeout_ms: 10_000,
            stats_window_ms: 60_000,
            max_events_per_10min: 300,
            max_batches_per_upload: 4,
            flush_threshold_bytes: 256 * 1024,
            max_batch_bytes: 1024 * 1024,
            log_severity: None,
            disabled_instruments: Vec::new(),
        }
    }
}

/// Entry point: the synchronous, never-blocking side SDKs push into.
///
/// Fail-open by design: [`emit`](Self::emit) cannot fail or block — when the queue is full the
/// oldest event is dropped and counted in [`stats`](Self::stats). Cheap to clone; every clone
/// feeds the same pipeline.
///
/// ```
/// # use std::sync::Arc;
/// # use livekit_telemetry::*;
/// # struct Discard;
/// # #[async_trait::async_trait]
/// # impl TelemetryTransport for Discard {
/// #     async fn send(&self, _: ExportRequest) -> Result<ExportResponse, ExportError> {
/// #         Ok(ExportResponse::accepted())
/// #     }
/// # }
/// # #[tokio::main(flavor = "current_thread")] async fn main() {
/// let (telemetry, exporter) = Telemetry::new(TelemetryConfig::default(), Arc::new(Discard));
/// tokio::spawn(exporter.run());
///
/// // One scope per room: the server URL and token route its records to the room's project.
/// let room = telemetry.begin_scope();
/// room.set_server("wss://my-project.livekit.cloud", "<participant token>");
/// room.emit_custom("checkout", vec![Attribute::new("plan", "pro")]);
/// telemetry.emit(TelemetryEvent::new("lk.ping"));
/// telemetry.shutdown().await;
/// # }
/// ```
#[derive(Clone)]
pub struct Telemetry {
    pub(crate) shared: Arc<Shared>,
    guard: Arc<Mutex<FloodGuard>>,
    commands: mpsc::UnboundedSender<Command>,
}

/// How much longer than `export_timeout_ms` [`Telemetry::shutdown`] waits for the exporter to
/// confirm it stopped.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(1);

/// Everything the synchronous side ([`Telemetry`]) and the exporter actor share.
pub(crate) struct Shared {
    pub store: Store,
    pub config: TelemetryConfig,
    pub cache: Arc<dyn BatchCache>,
    pub counters: Arc<Counters>,
    /// Read synchronously by the exporter, so a state pushed right before `emit` already governs
    /// the flush that follows.
    pub device: Mutex<Option<DeviceState>>,
    pub windows: Mutex<StatsWindows>,
    pub spans: Mutex<Spans>,
    /// The pipeline's own session: whatever is emitted outside a room session.
    pub process: Arc<ScopeState>,
    /// Attributes attached to every record of every session.
    pub global: Mutex<Vec<Attribute>>,
    /// Where each session's batches go, and with which token.
    pub destinations: Mutex<Destinations>,
    pub status: Mutex<TelemetryStatus>,
    /// Every room session, for the subscribe deadlines the exporter enforces.
    pub scopes: Mutex<Vec<Weak<ScopeState>>>,
    /// The queue crossed `flush_threshold_bytes` since the exporter last looked: export now.
    pub overflowed: AtomicBool,
    /// A destination, a token or a hold changed: re-evaluate what waits (nothing is encoded).
    pub released: AtomicBool,
    /// Test-only: runs after a capture's early opt-out check, before it takes the lock it commits
    /// under (to hold a producer exactly where a racing purge could slip in).
    #[cfg(test)]
    pub pause: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    /// Upload passes started, for tests that must see the exporter stay idle.
    #[cfg(test)]
    pub passes: std::sync::atomic::AtomicU64,
    /// Coalesced wake-ups for the exporter (see [`Exporter`]): notifications never queue up.
    pub wake: tokio::sync::Notify,
    /// The app went to the background since the exporter last looked.
    pub backgrounded: AtomicBool,
    /// The app opted out ([`Telemetry::purge`]). One-way: checked before anything is captured,
    /// written or sent, so a request still in flight cannot bring anything back.
    pub revoked: Arc<AtomicBool>,
}

impl Shared {
    /// The test hook between a capture's early check and its commit; nothing in production.
    fn before_commit(&self) {
        #[cfg(test)]
        {
            let pause = self.pause.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(pause) = pause {
                pause();
            }
        }
    }

    /// The queue crossed its threshold: wake the exporter to export now.
    pub fn overflow(&self) {
        self.overflowed.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }

    /// Something that held batches back changed: wake the exporter to re-evaluate.
    pub fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }

    /// Record a checkpoint inside an open span (`ws_open`, `join_recv`, …), stamped now.
    pub fn add_span_event(&self, span: u64, name: &str, attributes: Vec<Attribute>) {
        if self.revoked() {
            return;
        }
        self.spans.lock().unwrap_or_else(|e| e.into_inner()).add_event(span, name, attributes);
    }

    /// End a span with its outcome; `error_type` becomes `error.type` and the status message.
    /// The span is exported with the next batch. Ending twice, or an unknown handle, is a no-op.
    pub fn end_span(
        &self,
        span: u64,
        outcome: SpanOutcome,
        error_type: Option<String>,
        attributes: Vec<Attribute>,
    ) {
        if self.revoked() {
            return;
        }
        self.spans
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .end(span, outcome, error_type, attributes);
    }

    /// End every Room's pending subscribes (shutdown) or just forget them (opt-out): nothing
    /// is left referencing a span once the pipeline stops.
    pub fn end_pending_subscribes(&self, export: bool) {
        let scopes: Vec<Arc<ScopeState>> = {
            let scopes = self.scopes.lock().unwrap_or_else(|e| e.into_inner());
            scopes.iter().filter_map(|s| s.upgrade()).collect()
        };
        for scope in scopes {
            scope.end_subscribes(export);
        }
    }

    /// Device pressure, doubled again while WebRTC reports the encoder CPU-limited; capped at 4×.
    pub fn cadence_factor(&self) -> u32 {
        let device = self.device.lock().unwrap_or_else(|e| e.into_inner()).unwrap_or_default();
        let cpu = self.windows.lock().unwrap_or_else(|e| e.into_inner()).cpu_limited();
        (device.cadence_factor() * if cpu { 2 } else { 1 }).min(4)
    }

    pub fn revoked(&self) -> bool {
        self.revoked.load(Ordering::SeqCst)
    }

    /// Drop every record and batch, in memory and on disk, counted as `purged`. `false` when
    /// the storage refused to delete something: that data is still on disk.
    pub fn clear(&self) -> bool {
        self.end_pending_subscribes(false);
        let queued = self.store.clear();
        let spans = self.spans.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let windows = self.windows.lock().unwrap_or_else(|e| e.into_inner()).clear();
        // Count what really went, once: what was there before and is gone after (an
        // unreadable directory lists nothing, so nothing is counted for it).
        let before = self.cache.pending();
        let deleted = self.cache.clear();
        let after: std::collections::HashSet<String> = self.cache.pending().into_iter().collect();
        let cached: u64 = before
            .iter()
            .filter(|id| !after.contains(*id))
            .map(|id| crate::exporter::events_in(id))
            .sum();
        if let Err(err) = &deleted {
            log::warn!("opt-out: cached telemetry could not all be deleted ({err})");
        }
        Counters::add(&self.counters.purged, queued + spans + windows + cached);
        deleted.is_ok()
    }
}

/// Fixed-window cap on discrete events (the design doc's ~300 per 10 min).
struct FloodGuard {
    max: u32,
    window_start: Instant,
    count: u32,
    /// The first drop of a window logs; the rest are counted.
    warned: bool,
}

impl FloodGuard {
    const WINDOW: Duration = Duration::from_secs(10 * 60);

    fn new(max: u32) -> Self {
        Self { max, window_start: Instant::now(), count: 0, warned: false }
    }

    fn admit(&mut self) -> bool {
        if self.max == 0 {
            return true;
        }
        let now = Instant::now();
        if now.duration_since(self.window_start) >= Self::WINDOW {
            self.window_start = now;
            self.count = 0;
            self.warned = false;
        }
        if self.count >= self.max {
            if !self.warned {
                self.warned = true;
                log::warn!(
                    "flood: {} records in 10 min; dropping until the window moves",
                    self.max
                );
            }
            return false;
        }
        self.count += 1;
        true
    }
}

impl Telemetry {
    /// Build the pipeline with the cache the config asks for: a [`FileCache`] in `storage_dir`,
    /// or a [`MemoryCache`] when unset — or when the directory is unusable (logged, never an
    /// error). Spawn the returned [`Exporter`] with `exporter.run()` on your runtime.
    pub fn new(
        config: TelemetryConfig,
        transport: Arc<dyn TelemetryTransport>,
    ) -> (Self, Exporter) {
        let mut fell_back = false;
        let cache: Arc<dyn BatchCache> = match config.storage_dir.as_deref() {
            Some(dir) => match FileCache::open(dir, config.max_cache_bytes) {
                Ok(cache) => Arc::new(cache),
                Err(err) => {
                    log::warn!("cannot use storage dir {dir}: {err}; caching in memory");
                    fell_back = true;
                    Arc::new(MemoryCache::new(config.max_cache_bytes))
                }
            },
            None => Arc::new(MemoryCache::new(config.max_cache_bytes)),
        };
        let (telemetry, exporter) = Self::with_cache(config, transport, cache);
        if fell_back {
            // Reported like any other refused write: batches survive failures, not the process.
            Counters::add(&telemetry.shared.counters.cache_write_errors, 1);
        }
        (telemetry, exporter)
    }

    /// Build the pipeline around a caller-provided [`BatchCache`] (`storage_dir` is ignored).
    pub fn with_cache(
        mut config: TelemetryConfig,
        transport: Arc<dyn TelemetryTransport>,
        cache: Arc<dyn BatchCache>,
    ) -> (Self, Exporter) {
        add_sdk_resource(&mut config.resource, config.sdk.as_ref());
        let counters = Arc::new(Counters::default());
        let revoked = Arc::new(AtomicBool::new(false));
        let cache: Arc<dyn BatchCache> = Arc::new(SpillCache::new(
            cache,
            config.max_cache_bytes,
            counters.clone(),
            revoked.clone(),
        ));
        // Batches the cache evicted while opening (past the age or size limit) are lost too.
        let evicted: u64 =
            cache.take_evicted().iter().map(|id| crate::exporter::events_in(id)).sum();
        Counters::add(&counters.cache_full, evicted);
        let endpoint_override = std::env::var(ENDPOINT_OVERRIDE_ENV).ok();
        if let Some(endpoint) = endpoint_override.as_deref().filter(|e| !e.is_empty()) {
            log::info!("{ENDPOINT_OVERRIDE_ENV} set: every upload goes to {endpoint}");
        }
        // Unbounded, but only request/response commands travel here (flush, shutdown, purge),
        // each with a caller awaiting its answer: the queue is bounded by the callers waiting.
        // Notifications go through the coalescing `Shared::wake` instead.
        let (commands, receiver) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared {
            store: Store::new(
                config.max_queue_size.max(1) as usize,
                usize::try_from(config.flush_threshold_bytes.max(1)).unwrap_or(usize::MAX),
                counters.clone(),
            )
            .with_consent(revoked.clone()),
            cache,
            counters,
            device: Mutex::new(None),
            windows: Mutex::new(StatsWindows::with_consent(revoked.clone())),
            spans: Mutex::new(
                Spans::new(config.max_queue_size.max(1) as usize).with_consent(revoked.clone()),
            ),
            // One pipeline per process; sessions (rooms) carry their own trace ids. This is the
            // pipeline's own session, for everything emitted outside a room.
            process: ScopeState::new(),
            global: Mutex::new(Vec::new()),
            destinations: Mutex::new(Destinations::new(endpoint_override.as_deref())),
            status: Mutex::new(TelemetryStatus::Ok),
            scopes: Mutex::new(Vec::new()),
            revoked,
            #[cfg(test)]
            passes: Default::default(),
            #[cfg(test)]
            pause: Mutex::new(None),
            wake: tokio::sync::Notify::new(),
            overflowed: AtomicBool::new(false),
            released: AtomicBool::new(false),
            backgrounded: AtomicBool::new(false),
            config,
        });
        let guard = Arc::new(Mutex::new(FloodGuard::new(shared.config.max_events_per_10min)));
        let exporter = Exporter::new(shared.clone(), transport, receiver);
        (Self { shared, guard, commands }, exporter)
    }

    /// Route a room's records: its server URL names the project (LiveKit Cloud only), its token
    /// authorizes the upload. Called at connect and again with every refreshed token; cheap and
    /// idempotent. Everything cached for the project starts uploading once the token allows.
    pub(crate) fn set_server(&self, session: &ScopeState, url: &str, token: &str) {
        if session.same_server(url, token) {
            // A reconnect with the same pair still retries an ingest that answered 404 — one
            // map lookup, no parsing.
            let route = session.route();
            let revived = route.is_some_and(|host| {
                self.shared.destinations.lock().unwrap_or_else(|e| e.into_inner()).revive(&host)
            });
            if revived {
                self.shared.release();
            }
            return;
        }
        let host = self.shared.destinations.lock().unwrap_or_else(|e| e.into_inner()).set(
            url,
            token,
            &session.hex(),
        );
        let Some(host) = host else {
            log::warn!("server url has no host; this session's telemetry stays cached");
            return;
        };
        // A project change closes the session's RTC windows first: a window belongs to one project.
        if session.route().as_deref() != Some(host.as_str()) {
            self.with_windows_split(session, || true, || session.set_route(host.clone()));
        }
        // A new route or a new token may release what waits in the cache.
        self.shared.release();
    }

    /// The RTC stats window right now: `stats_window_ms` stretched by device pressure and a
    /// CPU-limited encoder (at most 4×).
    pub(crate) fn stats_window_ms(&self) -> u64 {
        self.shared.config.stats_window_ms.max(1) * u64::from(self.shared.cadence_factor())
    }

    /// An app handed over something over the limits (a custom event or attribute).
    pub(crate) fn count_invalid(&self) {
        Counters::add(&self.shared.counters.invalid, 1);
    }

    /// Nudge the exporter to re-read its deadlines (a subscribe started).
    pub(crate) fn wake(&self) {
        self.shared.wake.notify_one();
    }

    /// Point every upload at `endpoint`, as [`ENDPOINT_OVERRIDE_ENV`] does.
    #[cfg(test)]
    pub(crate) fn override_endpoint(&self, endpoint: &str) {
        *self.shared.destinations.lock().unwrap_or_else(|e| e.into_inner()) =
            Destinations::new(Some(endpoint));
    }

    /// Start a session — one room, one call — with its own trace id and attributes on this
    /// pipeline. Sessions do not need ending: a room's last record is simply its last.
    pub fn begin_scope(&self) -> Scope {
        let state = ScopeState::new();
        let mut scopes = self.shared.scopes.lock().unwrap_or_else(|e| e.into_inner());
        scopes.retain(|scope| scope.strong_count() > 0);
        scopes.push(Arc::downgrade(&state));
        Scope { telemetry: self.clone(), state }
    }

    /// A span in the pipeline's own trace: app-defined work outside any room, or the SDK before a
    /// room exists. Stamped now; `parent` nests it.
    pub fn start(&self, name: SpanName, parent: Option<Arc<Span>>) -> Arc<Span> {
        let parent = parent.and_then(|p| p.context()).map(|c| c.span_id);
        Span::bound(name, parent, self.clone(), &self.shared.process)
    }

    /// Queue an event or log record for export. Stamps it with the current time unless it
    /// carries one.
    ///
    /// A record with an empty `name` is a plain log line: only `Warn` and `Error` ones leave the
    /// device (design doc: debug/info logs never do). Discrete events are subject to the flood
    /// guard (`max_events_per_10min`); what it drops is counted as `rate_limited`.
    /// Something happened to the device mid-call (audio route, interruption, a denied permission):
    /// a process-level record with a display body, built here so every platform files it alike.
    pub fn device_event(&self, event: DeviceEvent) {
        self.emit_in(event.into_event(), &self.shared.process);
    }

    /// A captured log line. WebRTC only counts at error; the SDK and the core at the configured
    /// floor; the core's own telemetry module never (a rejected batch that produced a record that
    /// produced a batch would never end).
    pub fn log(&self, record: LogRecord) {
        if let Some(event) = self.log_event(record) {
            self.emit(event);
        }
    }

    /// The record as an event, or nothing when it is below the floor (WebRTC: error only) or is
    /// telemetry's own.
    pub(crate) fn log_event(&self, record: LogRecord) -> Option<TelemetryEvent> {
        let floor = match record.source {
            LogSource::WebRtc => self.log_severity().max(Severity::Error),
            _ => self.log_severity(),
        };
        if record.severity < floor {
            return None;
        }
        if record.source == LogSource::Ffi
            && record.logger.as_deref().is_some_and(|l| l.starts_with("livekit_telemetry"))
        {
            return None;
        }
        Some(record.into())
    }

    /// Queue an event or log record, filed under the session of the span it names (`span_id`),
    /// else under the pipeline's own process session.
    pub fn emit(&self, event: TelemetryEvent) {
        // A record emitted inside a room's span belongs to that room's session; anything else
        // is the process's own.
        let session = event
            .span_id
            .and_then(|id| self.shared.spans.lock().unwrap_or_else(|e| e.into_inner()).scope_of(id))
            .unwrap_or_else(|| self.shared.process.clone());
        self.emit_in(event, &session);
    }

    fn log_severity(&self) -> Severity {
        self.shared.config.log_severity.unwrap_or(Severity::Warn)
    }

    pub(crate) fn emit_in(&self, mut event: TelemetryEvent, session: &Arc<ScopeState>) {
        if event.name.is_empty() && event.severity < self.log_severity() {
            return;
        }
        if self.shared.revoked() {
            return;
        }
        if !self.collects(session) {
            Counters::add(&self.shared.counters.disabled, 1);
            return;
        }
        if !self.guard.lock().unwrap_or_else(|e| e.into_inner()).admit() {
            Counters::add(&self.shared.counters.rate_limited, 1);
            return;
        }
        if event.timestamp_ns.is_none() {
            event.timestamp_ns = Some(now_unix_nanos());
        }
        if self.shared.store.push(Queued::new(event, session.clone())) {
            self.shared.overflow();
        }
    }

    /// Queue a consumer-defined event, exported as `custom.<name>` (see
    /// [`TelemetryEvent::custom`]): the stringly-typed escape hatch next to the `lk.*`
    /// catalogue. Same flood guard, same pipeline.
    pub fn emit_custom(&self, name: &str, attributes: Vec<Attribute>) {
        Scope { telemetry: self.clone(), state: self.shared.process.clone() }
            .emit_custom(name, attributes);
    }

    /// Set a pipeline-wide attribute (a consumer's `enduser.id`, an `acme.tenant`), attached to
    /// every record of every session from now on unless the record — or its session — already
    /// carries the key. `None` removes it. Scope-level identity goes through
    /// [`Scope::set_attribute`].
    pub fn set_attribute(&self, key: &str, value: Option<AttributeValue>) {
        let mut global = self.shared.global.lock().unwrap_or_else(|e| e.into_inner());
        global.retain(|a| a.key != key);
        if let Some(value) = value {
            global.push(Attribute::new(key, value));
        }
    }

    /// The session's trace id as 32 hex characters — what every span and log record of this
    /// pipeline carries. Print it (`lkt_…`) so support can find the session.
    pub fn trace_id(&self) -> String {
        self.shared.process.hex()
    }

    /// Open a span: one attempt at an operation (`lk.connect`, `lk.publish`, …). Returns the
    /// handle to record checkpoints and to end it with; `parent` nests it under another open span.
    #[cfg(test)]
    pub(crate) fn begin_span(&self, name: &str, kind: SpanKind, parent: Option<u64>) -> u64 {
        self.begin_span_in(name, kind, parent, &self.shared.process)
    }

    pub(crate) fn begin_span_in(
        &self,
        name: &str,
        kind: SpanKind,
        parent: Option<u64>,
        session: &Arc<ScopeState>,
    ) -> u64 {
        // After the opt-out nothing opens: 0 is never a span id, so the span is detached.
        if self.shared.revoked() {
            return 0;
        }
        self.shared.before_commit();
        self.shared.spans.lock().unwrap_or_else(|e| e.into_inner()).begin_in(
            name,
            kind,
            parent,
            session.clone(),
        )
    }

    #[cfg(test)]
    pub(crate) fn add_span_event(&self, span: u64, name: &str, attributes: Vec<Attribute>) {
        self.shared.add_span_event(span, name, attributes);
    }

    #[cfg(test)]
    pub(crate) fn end_span(
        &self,
        span: u64,
        outcome: SpanOutcome,
        error_type: Option<String>,
        attributes: Vec<Attribute>,
    ) {
        self.shared.end_span(span, outcome, error_type, attributes);
    }

    /// Push one `getStats()` reading. Readings are windowed on device into `lk.rtc.stats.sample`
    /// events (see `stats_window_ms`); they never count against the flood guard.
    pub fn record_stats(&self, sample: RtcStatsSample) {
        self.record_stats_in(sample, &self.shared.process);
    }

    pub(crate) fn record_stats_in(&self, sample: RtcStatsSample, session: &Arc<ScopeState>) {
        if self.shared.revoked() || !self.collects(session) {
            return;
        }
        self.shared.before_commit();
        self.shared.windows.lock().unwrap_or_else(|e| e.into_inner()).record_in(sample, session);
    }

    /// Whether `session` still collects: not once its project is known to receive nothing
    /// (self-hosted server, no observability grant, disabled by the collector) — its records are
    /// dropped at the door, never written to the cache.
    fn collects(&self, session: &ScopeState) -> bool {
        let route = session.route();
        self.shared.destinations.lock().unwrap_or_else(|e| e.into_inner()).alive(route.as_deref())
    }

    /// Change something a session's RTC windows captured when they opened (its correlation
    /// attributes, its project), atomically with respect to readings, which record under the same
    /// lock: under the windows lock, check the change is `accept`ed, close the session's open
    /// windows, then `apply` it. Returns whether it was accepted.
    pub(crate) fn with_windows_split(
        &self,
        session: &ScopeState,
        accept: impl FnOnce() -> bool,
        apply: impl FnOnce(),
    ) -> bool {
        let closed = {
            let mut windows = self.shared.windows.lock().unwrap_or_else(|e| e.into_inner());
            if !accept() {
                return false;
            }
            let closed = windows.split(session);
            apply();
            closed
        };
        for window in closed {
            if self.shared.store.push(window) {
                self.shared.overflow();
            }
        }
        true
    }

    /// Close and forget the RTC windows of one track (or, with `None`, of the whole session):
    /// the last partial window is queued now instead of at the next tick.
    pub(crate) fn retire_stats(&self, session: &Arc<ScopeState>, track_sid: Option<&str>) {
        let closed = self
            .shared
            .windows
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retire(session, track_sid);
        for window in closed {
            // Windows bypass the flood guard: they are the pipeline's own, bounded output.
            if self.shared.store.push(window) {
                self.shared.overflow();
            }
        }
    }

    /// Tell the pipeline what the device looks like. Emits the `lk.device.*.changed` events for
    /// whatever differs from the last state (everything, the first time) and re-tunes the
    /// pipeline: pressure stretches the cadence up to 4× ([`DeviceState::cadence_factor`]), a
    /// constrained network or a nearly empty battery holds uploads
    /// ([`DeviceState::holds_uploads`]), and entering the background flushes once right away.
    pub fn set_device_state(&self, state: DeviceState) {
        let mut previous = self.shared.device.lock().unwrap_or_else(|e| e.into_inner());
        for event in state.change_events(previous.as_ref()) {
            self.emit(event);
        }
        let before = previous.replace(state).unwrap_or_default();
        drop(previous);
        // Only three things are worth a wake-up: entering the background (export now, the app
        // may be suspended), a hold changing (re-evaluate; nothing new is encoded for it) and the
        // cadence changing (the pending tick is re-derived: relief brings it forward).
        let offline = |s: &DeviceState| s.network == crate::NetworkType::Unavailable;
        if state.app_state == crate::AppState::Background && before.app_state != state.app_state {
            self.shared.backgrounded.store(true, Ordering::SeqCst);
            self.shared.wake.notify_one();
        } else if before.holds_uploads() != state.holds_uploads()
            || offline(&before) != offline(&state)
        {
            self.shared.release();
        } else if before.cadence_factor() != state.cadence_factor() {
            self.shared.wake.notify_one();
        }
    }

    /// Cache everything queued and upload the whole cache — no per-pass budget — as far as the
    /// holds and pauses in force allow. Returns when that pass is over.
    pub async fn flush(&self) {
        self.command(Command::Flush).await;
    }

    /// Flush, then stop the exporter, and return once it has stopped. The exporter gives the
    /// network `export_timeout_ms`, then cancels the request in flight and exits; what did not
    /// go out stays cached (with a [`FileCache`], for the next launch). Events emitted afterwards
    /// are never exported. If the exporter is not running at all, this gives up a second after
    /// that bound.
    pub async fn shutdown(&self) {
        let bound = Duration::from_millis(self.shared.config.export_timeout_ms.max(1));
        let _ = timeout(bound + SHUTDOWN_GRACE, self.command(Command::Shutdown)).await;
    }

    /// Pipeline health: drops by reason, uploads, cached batches. The same numbers ride to the
    /// backend as `lk.telemetry.report` events whenever something went wrong.
    pub fn stats(&self) -> TelemetryStats {
        TelemetryStats::new(
            self.shared.counters.snapshot(),
            self.shared.cache.pending().len() as u64,
            *self.shared.status.lock().unwrap_or_else(|e| e.into_inner()),
        )
    }

    /// Opt-out: stop the exporter without another upload and delete everything it holds — the
    /// queue, open and finished spans, RTC windows and every cached batch, on disk included.
    /// Events emitted afterwards go nowhere. Returns `false` when the storage refused to delete
    /// something (it is logged; the files stay until a later purge or configure succeeds).
    pub async fn purge(&self) -> bool {
        self.shared.revoked.store(true, Ordering::SeqCst);
        self.shared.clear();
        let bound = Duration::from_millis(self.shared.config.export_timeout_ms.max(1));
        let _ = timeout(bound, self.command(Command::Purge)).await;
        // Whatever an upload in flight put back while the exporter wound down.
        self.shared.clear()
    }

    /// A handle that can reach the exporter without keeping it alive (the opt-out uses it to
    /// cancel and await every generation).
    pub(crate) fn weak_commands(&self) -> mpsc::WeakUnboundedSender<Command> {
        self.commands.downgrade()
    }

    async fn command(&self, make: impl FnOnce(oneshot::Sender<()>) -> Command) {
        let (done, wait) = oneshot::channel();
        if self.commands.send(make(done)).is_ok() {
            let _ = wait.await;
        }
    }
}

/// Which LiveKit client SDK is reporting: `service.name` becomes `livekit-client-<sdk>`.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sdk {
    Swift,
    Android,
    Flutter,
    ReactNative,
    Unity,
    Rust,
}

impl Sdk {
    fn as_str(self) -> &'static str {
        match self {
            Self::Swift => "swift",
            Self::Android => "android",
            Self::Flutter => "flutter",
            Self::ReactNative => "react-native",
            Self::Unity => "unity",
            Self::Rust => "rust",
        }
    }
}

/// The reporting SDK and the device it runs on. Lowered to semconv: `service.name`,
/// `service.version`, `os.name`, `os.version`, `device.model.identifier`.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryResource {
    pub sdk: Sdk,
    pub sdk_version: String,
    pub os_name: String,
    pub os_version: String,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub device_model: Option<String>,
}

impl TelemetryResource {
    pub(crate) fn attributes(&self) -> Vec<Attribute> {
        let mut out = vec![
            Attribute::new("service.name", format!("livekit-client-{}", self.sdk.as_str())),
            Attribute::new("service.version", self.sdk_version.as_str()),
            Attribute::new("os.name", self.os_name.as_str()),
            Attribute::new("os.version", self.os_version.as_str()),
        ];
        if let Some(model) = &self.device_model {
            out.push(Attribute::new("device.model.identifier", model.as_str()));
        }
        out
    }
}

/// Lower the typed resource, then fill in the `telemetry.sdk.*` attributes and a fallback
/// `service.name`. Attributes already present (the open bag) win.
fn add_sdk_resource(resource: &mut Vec<Attribute>, sdk: Option<&TelemetryResource>) {
    for attribute in sdk.map(TelemetryResource::attributes).unwrap_or_default() {
        if !resource.iter().any(|a| a.key == attribute.key) {
            resource.push(attribute);
        }
    }
    let defaults = [
        ("service.name", "livekit-client"),
        ("telemetry.sdk.name", env!("CARGO_PKG_NAME")),
        ("telemetry.sdk.language", "rust"),
        ("telemetry.sdk.version", env!("CARGO_PKG_VERSION")),
    ];
    for (key, value) in defaults {
        if !resource.iter().any(|a| a.key == key) {
            resource.push(Attribute::new(key, value));
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::span::SpanKind;
    use crate::{ReconnectReason, RoomIdentity, SpanName, SpanStep};
    use std::{collections::VecDeque, fs, path::Path, sync::Mutex};

    #[tokio::test(start_paused = true)]
    async fn typed_spans_hold_uploads_while_connecting_and_export_when_ended() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        let session = telemetry.begin_scope();
        let span = session
            .start(SpanName::Reconnect { reason: ReconnectReason::SignalDisconnected }, None);
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert!(transport.sent().is_empty(), "an open reconnect holds uploads");
        span.step(SpanStep::Attempt { number: 1, full: false });
        span.end(SpanOutcome::Ok, None);
        assert!(span.context().is_some_and(|c| c.trace_id == session.trace_id()));
        telemetry.flush().await;
        assert!(transport.sent().iter().any(|r| r.url.contains("traces")), "the span is exported");
    }

    pub(crate) fn exported_spans(
        transport: &FakeTransport,
    ) -> Vec<crate::proto::opentelemetry::proto::trace::v1::Span> {
        transport
            .sent()
            .iter()
            .filter(|r| r.url.contains("traces"))
            .flat_map(|r| {
                ExportTraceServiceRequest::decode(&gunzip(&r.body)[..])
                    .expect("otlp")
                    .resource_spans
                    .into_iter()
                    .flat_map(|rs| rs.scope_spans.into_iter().flat_map(|ss| ss.spans))
            })
            .collect()
    }

    #[tokio::test(start_paused = true)]
    async fn the_subscribe_span_is_owned_by_the_core() {
        use crate::{SpanTrack, TrackSource};
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        let session = telemetry.begin_scope();
        let track = |sid: &str| SpanTrack {
            sid: Some(sid.into()),
            kind: TrackKind::Video,
            source: TrackSource::Camera,
            remote_identity: Some("bob".into()),
        };
        // Intent, confirmation, then the first inbound reading with bytes: ok.
        session.subscribe_started(track("TR_a"));
        session.subscribed(track("TR_a"));
        let mut empty = RtcStatsSample::new("TR_a", TrackKind::Video, StreamDirection::Inbound);
        empty.bytes = Some(0);
        session.record_stats(empty);
        let mut media = RtcStatsSample::new("TR_a", TrackKind::Video, StreamDirection::Inbound);
        media.bytes = Some(1_500);
        session.record_stats(media);
        // A second one nobody hears from — not a single reading, no other activity: the core's own
        // clock times it out. A third: unpublished before media.
        session.subscribe_started(track("TR_b"));
        session.subscribe_started(track("TR_c"));
        session.track_ended("TR_c");
        tokio::time::sleep(Scope::SUBSCRIBE_TIMEOUT + Duration::from_secs(1)).await;
        telemetry.flush().await;

        let spans = exported_spans(&transport);
        let by_sid = |sid: &str| {
            spans
                .iter()
                .find(|s| {
                    s.attributes.iter().any(|kv| {
                        kv.key == "lk.track.sid"
                            && kv.value.as_ref().and_then(|v| v.value.clone())
                                == Some(Value::StringValue(sid.into()))
                    })
                })
                .unwrap_or_else(|| panic!("span for {sid}"))
        };
        let ok = by_sid("TR_a");
        assert_eq!(ok.name, "lk.subscribe");
        assert_eq!(
            ok.events.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["subscribed", "first_media"]
        );
        assert!(ok.attributes.iter().any(|kv| kv.key == "lk.outcome"
            && kv.value.as_ref().and_then(|v| v.value.clone())
                == Some(Value::StringValue("ok".into()))));
        let timed_out = by_sid("TR_b");
        assert_eq!(
            timed_out.status.as_ref().map(|s| s.code),
            Some(status::StatusCode::Error as i32)
        );
        assert!(timed_out.attributes.iter().any(|kv| kv.key == "error.type"
            && kv.value.as_ref().and_then(|v| v.value.clone())
                == Some(Value::StringValue("timed_out".into()))));
        assert!(
            spans.iter().filter(|s| s.name == "lk.subscribe").count() >= 3,
            "cancelled exports too"
        );
        assert!(
            !spans.iter().any(|s| s.attributes.iter().any(|kv| kv.key == "lk.track.sid"
                && kv.value.as_ref().and_then(|v| v.value.clone())
                    == Some(Value::StringValue("TR_d".into())))),
            "still open"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn room_identity_and_resource_are_typed() {
        let transport = FakeTransport::scripted([]);
        let mut config = test_config();
        config.sdk = Some(TelemetryResource {
            sdk: Sdk::Swift,
            sdk_version: "2.16.0".into(),
            os_name: "iOS".into(),
            os_version: "19.0".into(),
            device_model: Some("iPhone17,1".into()),
        });
        let telemetry = start(config, transport.clone());
        let session = telemetry.begin_scope();
        session.set_room(RoomIdentity {
            sid: Some("RM_a".into()),
            name: Some("telemetry".into()),
            ..Default::default()
        });
        session.emit(TelemetryEvent::new("lk.ping"));
        telemetry.device_event(DeviceEvent::AudioInterruption { began: true });
        telemetry.flush().await;
        let sent = transport.sent();
        let logs: Vec<LogRecord> = sent.iter().flat_map(records).collect();
        let with_room =
            logs.iter().find(|r| attribute(r, "lk.room.sid").is_some()).expect("room record");
        assert_eq!(attribute(with_room, "lk.room.sid"), Some(Value::StringValue("RM_a".into())));
        assert!(logs.iter().any(|r| r.body.as_ref().and_then(|b| b.value.clone())
            == Some(Value::StringValue("audio interruption began".into()))));
        let decoded =
            ExportLogsServiceRequest::decode(&gunzip(&sent[0].body)[..]).expect("valid OTLP");
        let resource = decoded.resource_logs[0].resource.as_ref().expect("resource");
        let value = |key: &str| {
            resource
                .attributes
                .iter()
                .find(|kv| kv.key == key)
                .and_then(|kv| kv.value.as_ref())
                .and_then(|v| v.value.clone())
        };
        assert_eq!(value("service.name"), Some(Value::StringValue("livekit-client-swift".into())));
        assert_eq!(value("device.model.identifier"), Some(Value::StringValue("iPhone17,1".into())));
        assert!(value("telemetry.sdk.name").is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn log_records_apply_the_source_floor_and_carry_code_attributes() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        let line = |source, severity, logger: &str| crate::LogRecord {
            severity,
            source,
            body: "boom".into(),
            logger: Some(logger.into()),
            function: Some("connect()".into()),
            file: Some("Room.swift".into()),
            line: Some(42),
            timestamp_ns: None,
            span_id: None,
        };
        telemetry.log(line(LogSource::WebRtc, Severity::Warn, "sctp.cc"));
        telemetry.log(line(LogSource::Sdk, Severity::Info, "Room"));
        telemetry.log(line(LogSource::Ffi, Severity::Error, "livekit_telemetry::exporter"));
        telemetry.log(line(LogSource::Sdk, Severity::Warn, "Room"));
        telemetry.log(line(LogSource::WebRtc, Severity::Error, "sctp.cc"));
        telemetry.flush().await;
        let sent = transport.sent();
        assert_eq!(sent.len(), 1);
        let logs = records(&sent[0]);
        assert_eq!(logs.len(), 2, "sdk warn + webrtc error; not webrtc warn, sdk info, own module");
        let sdk = &logs[0];
        assert_eq!(attribute(sdk, "lk.log.source"), Some(Value::StringValue("sdk".into())));
        assert_eq!(attribute(sdk, "lk.log.logger"), Some(Value::StringValue("Room".into())));
        assert_eq!(
            attribute(sdk, "code.function.name"),
            Some(Value::StringValue("connect()".into()))
        );
        assert_eq!(attribute(sdk, "code.line.number"), Some(Value::IntValue(42)));
        assert_eq!(
            sdk.body.as_ref().and_then(|b| b.value.clone()),
            Some(Value::StringValue("boom".into()))
        );
        assert_eq!(attribute(&logs[1], "lk.log.source"), Some(Value::StringValue("webrtc".into())));
    }

    use prost::Message;

    use super::*;
    use crate::{
        cache::temp_dir,
        proto::opentelemetry::proto::{
            collector::{logs::v1::ExportLogsServiceRequest, trace::v1::ExportTraceServiceRequest},
            common::v1::any_value::Value,
            logs::v1::LogRecord,
            trace::v1::{span, status},
        },
        AppState, ExportError, ExportRequest, ExportResponse, SpanOutcome, StreamDirection,
        ThermalState, TrackKind,
    };

    #[derive(Default)]
    pub(crate) struct FakeTransport {
        requests: Mutex<Vec<ExportRequest>>,
        script: Mutex<VecDeque<Result<ExportResponse, ExportError>>>,
    }

    impl FakeTransport {
        pub(crate) fn scripted(
            results: impl IntoIterator<Item = Result<ExportResponse, ExportError>>,
        ) -> Arc<Self> {
            Arc::new(Self {
                script: Mutex::new(results.into_iter().collect()),
                ..Default::default()
            })
        }
        pub(crate) fn sent(&self) -> Vec<ExportRequest> {
            self.requests.lock().expect("lock").clone()
        }

        /// Queue more answers.
        pub(crate) fn then(
            &self,
            results: impl IntoIterator<Item = Result<ExportResponse, ExportError>>,
        ) {
            self.script.lock().expect("lock").extend(results);
        }
    }

    #[async_trait::async_trait]
    impl TelemetryTransport for FakeTransport {
        async fn send(&self, request: ExportRequest) -> Result<ExportResponse, ExportError> {
            self.requests.lock().expect("lock").push(request);
            let scripted = self.script.lock().expect("lock").pop_front();
            scripted.unwrap_or_else(|| Ok(ExportResponse::accepted()))
        }
    }

    /// An HTTP answer with this status, headers and body.
    pub(crate) fn answer(
        status: u16,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Result<ExportResponse, ExportError> {
        Ok(ExportResponse {
            status,
            headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            body: body.to_vec(),
        })
    }

    /// The old defaults (1 s export, 15 s windows): the pipeline mechanics tests reason in
    /// those; the conservative production defaults have their own tests.
    pub(crate) fn test_config() -> TelemetryConfig {
        TelemetryConfig { flush_interval_ms: 1000, stats_window_ms: 15_000, ..Default::default() }
    }

    pub(crate) fn gunzip(body: &[u8]) -> Vec<u8> {
        use std::io::Read;
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(body).read_to_end(&mut out).expect("gzip body");
        out
    }

    pub(crate) fn offline() -> Result<ExportResponse, ExportError> {
        Err(ExportError::Retryable { reason: "offline".into(), retry_after_ms: None })
    }

    pub(crate) fn offline_forever() -> impl Iterator<Item = Result<ExportResponse, ExportError>> {
        std::iter::repeat_with(offline).take(64)
    }

    pub(crate) fn pipeline(transport: Arc<FakeTransport>) -> Telemetry {
        start(test_config(), transport)
    }

    pub(crate) fn persisted_pipeline(transport: Arc<FakeTransport>, dir: &Path) -> Telemetry {
        let mut config = test_config();
        config.storage_dir = Some(dir.to_string_lossy().into_owned());
        start(config, transport)
    }

    /// A running pipeline whose uploads all go to `http://collector` (the test override).
    pub(crate) fn start(config: TelemetryConfig, transport: Arc<FakeTransport>) -> Telemetry {
        let telemetry = start_cloud(config, transport);
        telemetry.override_endpoint("http://collector");
        telemetry
    }

    /// A running pipeline with LiveKit Cloud routing: nothing uploads before `set_server`.
    pub(crate) fn start_cloud(config: TelemetryConfig, transport: Arc<FakeTransport>) -> Telemetry {
        let (telemetry, exporter) = Telemetry::new(config, transport);
        tokio::spawn(exporter.run());
        telemetry
    }

    pub(crate) fn files_in(dir: &Path) -> usize {
        fs::read_dir(dir).map(|d| d.count()).unwrap_or(0)
    }

    pub(crate) fn records(request: &ExportRequest) -> Vec<LogRecord> {
        let decoded =
            ExportLogsServiceRequest::decode(&gunzip(&request.body)[..]).expect("valid OTLP");
        decoded.resource_logs[0].scope_logs[0].log_records.clone()
    }

    pub(crate) fn event_names(request: &ExportRequest) -> Vec<String> {
        records(request).iter().map(|r| r.event_name.clone()).collect()
    }

    pub(crate) fn attribute(record: &LogRecord, key: &str) -> Option<Value> {
        record.attributes.iter().find(|kv| kv.key == key)?.value.as_ref()?.value.clone()
    }

    #[tokio::test(start_paused = true)]
    async fn batches_events_into_one_otlp_request() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        for _ in 0..3 {
            telemetry.emit(TelemetryEvent::new("lk.ping"));
        }
        telemetry.flush().await;

        let sent = transport.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].url, "http://collector/v1/logs");
        assert_eq!(sent[0].headers["Content-Type"], "application/x-protobuf");
        assert_eq!(event_names(&sent[0]), ["lk.ping"; 3]);
        let decoded =
            ExportLogsServiceRequest::decode(&gunzip(&sent[0].body)[..]).expect("valid OTLP");
        let resource = decoded.resource_logs[0].resource.as_ref().expect("resource");
        assert!(resource.attributes.iter().any(|kv| kv.key == "telemetry.sdk.name"));
        assert_eq!(telemetry.stats().dropped, 0);
        assert_eq!(telemetry.stats().uploads_sent, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn failed_upload_waits_in_memory_and_is_retried_after_backoff() {
        let transport = FakeTransport::scripted([offline()]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 1, "one attempt: retrying is the backoff's job");
        assert_eq!(telemetry.stats().dropped, 0, "kept in the memory cache, not dropped");
        assert_eq!(telemetry.stats().upload_failures, 1);
        assert_eq!(telemetry.stats().cached_batches, 1);

        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 1, "backoff: no upload right away");

        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(transport.sent().len(), 2, "retried once the backoff elapsed, before the tick");
        assert_eq!(event_names(&transport.sent()[1]), ["lk.ping"]);
        assert_eq!(telemetry.stats().cached_batches, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn self_telemetry_report_rides_along_after_problems() {
        let transport = FakeTransport::scripted([offline(), offline(), offline()]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        tokio::time::sleep(Duration::from_secs(61)).await; // backoff over, batch uploads

        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        let sent = transport.sent();
        let last = &sent[sent.len() - 1];
        assert_eq!(event_names(last), ["lk.ping", "lk.telemetry.report"]);
        let report = &records(last)[1];
        assert_eq!(attribute(report, "lk.telemetry.uploads.failed"), Some(Value::IntValue(3)));
        assert_eq!(attribute(report, "lk.telemetry.uploads.sent"), Some(Value::IntValue(1)));
        assert_eq!(attribute(report, "lk.telemetry.cache.batches"), Some(Value::IntValue(0)));
        assert_eq!(attribute(report, "lk.telemetry.dropped.queue_full"), None, "zeros omitted");

        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        let sent = transport.sent();
        assert_eq!(event_names(&sent[sent.len() - 1]), ["lk.ping"], "nothing new to report");
    }

    #[tokio::test(start_paused = true)]
    async fn queue_overflow_is_counted_by_reason() {
        let mut config = test_config();
        config.max_queue_size = 1;
        let telemetry = start(config, FakeTransport::scripted([]));
        for _ in 0..3 {
            telemetry.emit(TelemetryEvent::new("lk.ping"));
        }
        let stats = telemetry.stats();
        assert_eq!(stats.dropped_queue_full, 2);
        assert_eq!(stats.dropped, 2);
    }

    #[tokio::test(start_paused = true)]
    async fn rejected_batch_is_dropped_without_retry() {
        let transport =
            FakeTransport::scripted([Err(ExportError::Rejected { reason: "400".into() })]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;

        assert_eq!(transport.sent().len(), 1);
        assert_eq!(telemetry.stats().dropped_rejected, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn batch_is_written_before_upload_and_replayed_on_next_start() {
        let dir = temp_dir("replay");
        let first_transport = FakeTransport::scripted(offline_forever());
        let first = persisted_pipeline(first_transport.clone(), &dir);
        first.emit(TelemetryEvent::new("lk.ping"));
        first.flush().await;
        assert_eq!(first_transport.sent().len(), 1);
        assert_eq!(first.stats().dropped, 0);
        assert_eq!(files_in(&dir), 1, "written before the first attempt, kept after failure");

        let second_transport = FakeTransport::scripted([]);
        let second = persisted_pipeline(second_transport.clone(), &dir);
        second.flush().await;
        let sent = second_transport.sent();
        assert_eq!(sent.len(), 1, "replayed on start");
        assert_eq!(event_names(&sent[0]), ["lk.ping"]);
        assert_eq!(files_in(&dir), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn throttling_holds_uploads_and_keeps_collecting() {
        let dir = temp_dir("throttle");
        let throttled =
            Err(ExportError::Retryable { reason: "429".into(), retry_after_ms: Some(5_000) });
        let transport = FakeTransport::scripted([throttled]);
        let telemetry = persisted_pipeline(transport.clone(), &dir);
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 1, "no retries on Retry-After");
        assert_eq!(files_in(&dir), 1, "the throttled batch stays cached");
        assert_eq!(telemetry.stats().dropped, 0);

        // A hold pauses uploads, not collection: what happens during the quiet window is the
        // part an operator most wants afterwards.
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert_eq!(files_in(&dir), 2, "events inside the window are cached too");
        assert_eq!(telemetry.stats().dropped, 0, "and nothing is thrown away");

        tokio::time::sleep(Duration::from_secs(6)).await;
        assert_eq!(transport.sent().len(), 3, "both cached batches upload after Retry-After");
        assert_eq!(files_in(&dir), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    /// The cache is the floor under a hold: outlast it and the oldest batches go, counted apart
    /// from an ordinary overflow so the report says *why* the session has a hole.
    #[tokio::test(start_paused = true)]
    async fn a_hold_longer_than_the_cache_reports_what_it_cost() {
        let dir = temp_dir("throttle-overflow");
        let throttled =
            Err(ExportError::Retryable { reason: "429".into(), retry_after_ms: Some(60_000) });
        let transport = FakeTransport::scripted([throttled]);
        let mut config = test_config();
        config.storage_dir = Some(dir.to_string_lossy().into_owned());
        config.max_cache_bytes = 700; // a couple of batches, so the hold overruns it quickly
        let telemetry = start(config, transport.clone());

        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert_eq!(telemetry.stats().dropped, 0, "the first batch is cached, not dropped");

        for _ in 0..8 {
            telemetry.emit(TelemetryEvent::new("lk.ping"));
            telemetry.flush().await;
        }
        let stats = telemetry.stats();
        assert!(stats.dropped_throttled > 0, "evictions inside a hold are attributed to it");
        assert_eq!(stats.dropped, stats.dropped_throttled, "and to nothing else");
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_offline_keeps_queue_on_disk() {
        let dir = temp_dir("spill");
        let transport = FakeTransport::scripted(offline_forever());
        let telemetry = persisted_pipeline(transport.clone(), &dir);
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.shutdown().await;

        // One file, or two when the first tick shipped the ping before shutdown added the summary.
        let files = files_in(&dir);
        assert!(
            (1..=2).contains(&files),
            "cached before the network was tried, kept after: {files}"
        );
        assert_eq!(telemetry.stats().dropped, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn custom_cache_is_used_as_is() {
        let cache = Arc::new(MemoryCache::new(1 << 20));
        let transport = FakeTransport::scripted(offline_forever());
        let (telemetry, exporter) = Telemetry::with_cache(test_config(), transport, cache.clone());
        tokio::spawn(exporter.run());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        let pending = cache.pending();
        assert_eq!(pending.len(), 1);
        assert!(pending[0].contains("-1-l-"), "id carries count and signal: {}", pending[0]);
    }

    #[tokio::test(start_paused = true)]
    async fn device_state_emits_change_events_and_stretches_cadence() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        telemetry.set_device_state(DeviceState {
            thermal: ThermalState::Critical,
            ..DeviceState::default()
        });
        telemetry.flush().await;
        let sent = transport.sent();
        assert_eq!(sent.len(), 1);
        let names = event_names(&sent[0]);
        assert!(names.contains(&"lk.device.thermal.changed".to_owned()), "{names:?}");
        assert_eq!(
            names.len(),
            4,
            "initial value for every known field (battery, low power unknown)"
        );
        let thermal = records(&sent[0])
            .into_iter()
            .find(|r| r.event_name == "lk.device.thermal.changed")
            .expect("thermal event");
        assert_eq!(
            attribute(&thermal, "lk.device.thermal.state"),
            Some(Value::StringValue("critical".into()))
        );

        // 1 s base interval × 4 under critical thermal pressure.
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(transport.sent().len(), 1, "not yet: cadence stretched to 4 s");
        tokio::time::sleep(Duration::from_millis(2_500)).await;
        assert_eq!(transport.sent().len(), 2, "exported on the stretched tick");
    }

    #[tokio::test(start_paused = true)]
    async fn requests_are_gzipped_and_low_priority() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        let sent = transport.sent();
        assert_eq!(sent[0].headers["Content-Encoding"], "gzip");
        assert_eq!(sent[0].headers["Priority"], "u=7");
        assert_eq!(event_names(&sent[0]), ["lk.ping"]);
    }

    #[tokio::test(start_paused = true)]
    async fn uploads_hold_while_connecting_but_never_beyond_the_cap() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        let connect = telemetry.begin_span("lk.connect", SpanKind::Client, None);
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert!(transport.sent().is_empty(), "the uplink belongs to signaling and ICE");
        assert_eq!(telemetry.stats().cached_batches, 1, "…but the batch is safely cached");

        tokio::time::sleep(Duration::from_secs(301)).await;
        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 1, "held 5 min: one batch goes out regardless");
        assert_eq!(telemetry.stats().holds_capped, 1, "…and the starvation is counted");

        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.end_span(connect, SpanOutcome::Ok, None, Vec::new());
        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 3, "connected: the ping and the connect span ship");
    }

    #[tokio::test(start_paused = true)]
    async fn bandwidth_limitation_does_not_hold_uploads() {
        // WebRTC reports `bandwidth` for minutes during ramp-up and for as long as an encoder
        // stalls; holding on it starved a real device of uploads for 8 minutes.
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        let limited = |ms| RtcStatsSample {
            quality_limitation_bandwidth_ms: Some(ms),
            ..RtcStatsSample::new("TR_1", TrackKind::Video, StreamDirection::Outbound)
        };
        telemetry.record_stats(limited(0));
        telemetry.record_stats(limited(800));
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 1, "yielding to media is the transport's job");
    }

    #[tokio::test(start_paused = true)]
    async fn device_holds_uploads_and_a_backlog_replays_within_the_budget() {
        let transport = FakeTransport::scripted([]);
        let mut config = test_config();
        config.max_batches_per_upload = 2;
        let telemetry = start(config, transport.clone());
        let call = telemetry.begin_scope();
        call.set_server("wss://p.livekit.cloud", "token"); // the budget applies next to a call
        telemetry.set_device_state(DeviceState { network_constrained: true, ..Default::default() });
        for _ in 0..5 {
            telemetry.emit(TelemetryEvent::new("lk.ping"));
            telemetry.flush().await;
        }
        assert!(transport.sent().is_empty(), "Low Data Mode: record, do not upload");
        assert_eq!(telemetry.stats().cached_batches, 5);

        // Back to normal: the change wakes the exporter, which replays within the budget.
        telemetry.set_device_state(DeviceState::default());
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert_eq!(transport.sent().len(), 2, "two batches per pass next to a live call");
        tokio::time::sleep(Duration::from_millis(1000)).await; // the next tick
        assert_eq!(transport.sent().len(), 4);
        // An explicit flush drains the rest (the change event made a sixth batch).
        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 6, "flush has no per-pass budget");
        telemetry.shutdown().await;
        assert_eq!(transport.sent().len(), 7, "shutdown drains too (+ the summary)");
    }

    struct Hanging;

    #[async_trait::async_trait]
    impl TelemetryTransport for Hanging {
        async fn send(&self, _: ExportRequest) -> Result<ExportResponse, ExportError> {
            std::future::pending().await
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_full_queue_flushes_before_the_tick() {
        let transport = FakeTransport::scripted([]);
        let mut config = test_config();
        config.flush_interval_ms = 60_000;
        config.flush_threshold_bytes = 10_000;
        let telemetry = start(config, transport.clone());
        tokio::time::sleep(Duration::from_millis(1)).await; // the immediate first tick passes
        for _ in 0..3 {
            telemetry.emit(TelemetryEvent::new("big").with_body("x".repeat(4_000)));
        }
        tokio::time::sleep(Duration::from_millis(1)).await; // the wake-up is processed
        let sent = transport.sent();
        assert_eq!(sent.len(), 1, "exported on crossing the byte threshold, a minute early");
        assert_eq!(records(&sent[0]).len(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn requests_stay_under_the_byte_cap() {
        let transport = FakeTransport::scripted([]);
        let mut config = test_config();
        config.max_batch_bytes = 10_000;
        config.max_batches_per_upload = 10;
        let telemetry = start(config, transport.clone());
        for _ in 0..5 {
            telemetry.emit(TelemetryEvent::new("big").with_body("x".repeat(4_000)));
        }
        telemetry.flush().await;
        let sent = transport.sent();
        assert_eq!(sent.len(), 3, "5 × ~4 KB under a 10 KB cap: 2 + 2 + 1");
        assert!(sent.iter().all(|request| records(request).len() <= 2));
    }

    #[tokio::test(start_paused = true)]
    async fn custom_events_are_namespaced() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit_custom("acme.checkout", vec![Attribute::new("acme.step", 3i64)]);
        telemetry.emit_custom("custom.already", Vec::new());
        telemetry.flush().await;
        let sent = transport.sent();
        assert_eq!(event_names(&sent[0]), ["custom.acme.checkout", "custom.already"]);
        let record = records(&sent[0]).remove(0);
        assert_eq!(attribute(&record, "acme.step"), Some(Value::IntValue(3)));
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_leaves_a_session_summary() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        telemetry.shutdown().await;
        let sent = transport.sent();
        assert_eq!(sent.len(), 2, "the summary is its own batch when nothing else is queued");
        let report = records(&sent[1]).remove(0);
        assert_eq!(report.event_name, "lk.telemetry.report");
        assert_eq!(attribute(&report, "lk.telemetry.uploads.sent"), Some(Value::IntValue(1)));
        assert!(
            matches!(attribute(&report, "lk.telemetry.uploads.bytes"), Some(Value::IntValue(n)) if n > 0),
            "bytes on the wire are reported"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cache_eviction_is_counted_as_a_drop() {
        let transport = FakeTransport::scripted([offline(), offline(), offline()]);
        // Room for exactly one batch: a second push evicts the first.
        let (telemetry, exporter) =
            Telemetry::with_cache(test_config(), transport.clone(), Arc::new(MemoryCache::new(1)));
        tokio::spawn(exporter.run());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await; // fails: cached, upload paused
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        let stats = telemetry.stats();
        assert_eq!(stats.cached_batches, 1);
        assert_eq!(stats.dropped_cache_full, 1, "the evicted ping is counted, not silently lost");
    }

    #[tokio::test(start_paused = true)]
    async fn timeouts_are_counted_apart_from_failures() {
        let (telemetry, exporter) = Telemetry::new(test_config(), Arc::new(Hanging));
        telemetry.override_endpoint("http://collector");
        tokio::spawn(exporter.run());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await; // one attempt, bounded by export_timeout under paused time
        let stats = telemetry.stats();
        assert_eq!(stats.upload_timeouts, 1);
        assert_eq!(stats.status, TelemetryStatus::Paused, "backing off");
        assert_eq!(stats.upload_failures, 0);
        assert_eq!(stats.cached_batches, 1, "kept for the next attempt");
    }

    #[tokio::test(start_paused = true)]
    async fn sessions_have_their_own_trace_and_attributes() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        telemetry.set_attribute("acme.tenant", Some("t1".into()));
        let a = telemetry.begin_scope();
        let b = telemetry.begin_scope();
        assert_ne!(a.trace_id(), b.trace_id());
        assert_ne!(a.trace_id(), telemetry.trace_id(), "the process has its own session");
        a.set_room(RoomIdentity { sid: Some("RM_a".into()), ..Default::default() });
        b.set_room(RoomIdentity { sid: Some("RM_b".into()), ..Default::default() });
        let span = a.start(SpanName::Connect, None);
        let span_id = span.context().expect("bound").span_id;
        a.emit(TelemetryEvent::new("lk.ping"));
        b.emit(TelemetryEvent::new("lk.ping"));
        // A warn record from the SDK logger, inside room A's connect: no session handle, just
        // the ambient span id — the core files it under A.
        telemetry.emit(
            TelemetryEvent::new("").with_severity(Severity::Warn).with_body("hmm").in_span(span_id),
        );
        telemetry.emit(TelemetryEvent::new("lk.device.thermal.changed"));
        span.end(SpanOutcome::Ok, None);
        telemetry.flush().await;

        let sent = transport.sent();
        assert_eq!(sent.len(), 4, "a batch per owner: A's span, A's logs, B's, the process's");
        let logs: Vec<LogRecord> =
            sent.iter().filter(|r| r.url.ends_with("logs")).flat_map(records).collect();
        assert_eq!(logs.len(), 4);
        assert_eq!(hex(&logs[0].trace_id), a.trace_id());
        assert_eq!(attribute(&logs[0], "lk.room.sid"), Some(Value::StringValue("RM_a".into())));
        assert_eq!(attribute(&logs[0], "session.id"), Some(Value::StringValue(a.trace_id())));
        assert_eq!(hex(&logs[1].trace_id), a.trace_id(), "resolved through the span");
        assert_eq!(hex(&logs[2].trace_id), b.trace_id());
        assert_eq!(attribute(&logs[2], "lk.room.sid"), Some(Value::StringValue("RM_b".into())));
        assert_eq!(hex(&logs[3].trace_id), telemetry.trace_id(), "device state: process session");
        assert_eq!(attribute(&logs[3], "lk.room.sid"), None);
        assert!(
            logs.iter()
                .all(|r| attribute(r, "acme.tenant") == Some(Value::StringValue("t1".into()))),
            "a pipeline-wide attribute reaches every session"
        );
        let traces_request = sent.iter().find(|r| r.url.ends_with("traces")).expect("traces");
        let traces =
            ExportTraceServiceRequest::decode(&gunzip(&traces_request.body)[..]).expect("otlp");
        let otlp_span = &traces.resource_spans[0].scope_spans[0].spans[0];
        assert_eq!(hex(&otlp_span.trace_id), a.trace_id());
        assert!(otlp_span.attributes.iter().any(|a| a.key == "lk.room.sid"));

        // A record that names a span which has already ended — and been exported — is still that
        // session's: the SDK's log path hops threads, the span does not wait for it.
        telemetry.emit(
            TelemetryEvent::new("")
                .with_severity(Severity::Error)
                .with_body("late")
                .in_span(span_id),
        );
        telemetry.flush().await;
        let late = &records(transport.sent().last().expect("sent"))[0];
        assert_eq!(hex(&late.trace_id), a.trace_id(), "filed under the ended span's session");
        assert_eq!(late.span_id, span_id.to_be_bytes().to_vec());
    }

    #[tokio::test(start_paused = true)]
    async fn uploads_wait_for_a_destination() {
        let transport = FakeTransport::scripted([]);
        let telemetry = start_cloud(test_config(), transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        tokio::time::sleep(Duration::from_secs(600)).await;
        assert!(transport.sent().is_empty(), "no destination: nothing leaves, no hold cap either");
        assert_eq!(telemetry.stats().cached_batches, 1);
        assert_eq!(telemetry.stats().status, TelemetryStatus::Waiting);

        let token = crate::destination::tests::granted(3600);
        let session = telemetry.begin_scope();
        session.set_server("wss://x.livekit.cloud", &token);
        tokio::time::sleep(Duration::from_millis(1)).await;
        let sent = transport.sent();
        assert_eq!(sent.len(), 1, "cached batches ship as soon as the destination is known");
        assert_eq!(sent[0].url, "https://x.livekit.cloud/observability/client/logs/otlp/v0");
        assert_eq!(sent[0].headers["Authorization"], format!("Bearer {token}"));
        session.start(SpanName::Publish, None).end(SpanOutcome::Ok, None);
        telemetry.flush().await;
        assert_eq!(
            transport.sent()[1].url,
            "https://x.livekit.cloud/observability/client/traces/otlp/v0"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn debug_and_info_logs_never_leave_the_device() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("").with_severity(Severity::Info).with_body("noise"));
        telemetry.emit(TelemetryEvent::new("").with_severity(Severity::Error).with_body("boom"));
        telemetry.flush().await;

        let sent = transport.sent();
        let records = records(&sent[0]);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].event_name, "", "a log line, not an event");
        assert_eq!(records[0].severity_text, "ERROR");
        assert_eq!(
            records[0].body.as_ref().and_then(|b| b.value.clone()),
            Some(Value::StringValue("boom".into()))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn flood_guard_caps_events_but_not_stats_windows() {
        let mut config = test_config();
        config.max_events_per_10min = 1;
        config.stats_window_ms = 1_000;
        let transport = FakeTransport::scripted([]);
        let telemetry = start(config, transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.record_stats(RtcStatsSample::new(
            "TR_1",
            TrackKind::Audio,
            StreamDirection::Inbound,
        ));
        assert_eq!(telemetry.stats().dropped_rate_limited, 1);

        tokio::time::sleep(Duration::from_millis(1_100)).await; // stats window closes
        telemetry.flush().await;
        let names: Vec<String> = transport.sent().iter().flat_map(event_names).collect();
        assert_eq!(names.iter().filter(|n| *n == "lk.ping").count(), 1);
        assert!(names.contains(&"lk.rtc.stats.sample".to_owned()), "{names:?}");
        assert!(names.contains(&"lk.telemetry.report".to_owned()), "rate limiting is reported");
    }

    #[tokio::test(start_paused = true)]
    async fn stats_readings_are_windowed_into_one_event() {
        let mut config = test_config();
        config.stats_window_ms = 2_000;
        let transport = FakeTransport::scripted([]);
        let telemetry = start(config, transport.clone());
        for (bytes, jitter) in [(100, 1.0), (200, 3.0), (300, 2.0)] {
            let mut sample =
                RtcStatsSample::new("TR_1", TrackKind::Video, StreamDirection::Inbound);
            sample.bytes = Some(bytes);
            sample.jitter_ms = Some(jitter);
            sample.codec = Some("video/VP8".into());
            telemetry.record_stats(sample);
        }
        telemetry.flush().await;
        assert!(transport.sent().is_empty(), "windows do not flush early");

        tokio::time::sleep(Duration::from_millis(2_100)).await;
        telemetry.flush().await;
        let sent = transport.sent();
        let window = records(&sent[0])
            .into_iter()
            .find(|r| r.event_name == "lk.rtc.stats.sample")
            .expect("window");
        assert_eq!(attribute(&window, "lk.track.kind"), Some(Value::StringValue("video".into())));
        assert_eq!(
            attribute(&window, "lk.rtc.codec"),
            Some(Value::StringValue("video/VP8".into()))
        );
        assert_eq!(attribute(&window, "lk.rtc.bytes"), Some(Value::IntValue(300)));
        assert_eq!(attribute(&window, "lk.rtc.samples"), Some(Value::IntValue(3)));
        assert_eq!(attribute(&window, "lk.rtc.jitter_ms.avg"), Some(Value::DoubleValue(2.0)));
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_closes_open_stats_windows() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        telemetry.record_stats(RtcStatsSample::new(
            "TR_1",
            TrackKind::Audio,
            StreamDirection::Outbound,
        ));
        telemetry.shutdown().await;
        let names: Vec<String> = transport.sent().iter().flat_map(event_names).collect();
        assert_eq!(
            names,
            ["lk.rtc.stats.sample", "lk.telemetry.report"],
            "window + shutdown summary"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn session_attributes_are_attached_to_every_record() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        telemetry.set_attribute("lk.room.sid", Some("RM_1".into()));
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.emit(TelemetryEvent::new("lk.ping").with_attribute("lk.room.sid", "RM_override"));
        telemetry.flush().await;
        let first = records(&transport.sent()[0]);
        assert_eq!(attribute(&first[0], "lk.room.sid"), Some(Value::StringValue("RM_1".into())));
        assert_eq!(
            attribute(&first[1], "lk.room.sid"),
            Some(Value::StringValue("RM_override".into())),
            "an explicit attribute wins"
        );
        telemetry.set_attribute("lk.room.sid", None);
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert_eq!(attribute(&records(&transport.sent()[1])[0], "lk.room.sid"), None);
    }

    #[tokio::test(start_paused = true)]
    async fn spans_export_as_traces_under_the_session_trace_id() {
        let transport = FakeTransport::scripted([]);
        let telemetry = start(test_config(), transport.clone());
        telemetry.override_endpoint("http://c/observability/client/logs/otlp/v0");
        let connect = telemetry.begin_span("lk.connect", SpanKind::Client, None);
        telemetry.add_span_event(connect, "ws_open", vec![]);
        telemetry.emit(
            TelemetryEvent::new("")
                .with_severity(Severity::Error)
                .with_body("boom")
                .in_span(connect),
        );
        telemetry.end_span(
            connect,
            SpanOutcome::Error,
            Some("timeout".into()),
            vec![Attribute::new("lk.connect.attempt", 1i64)],
        );
        telemetry.flush().await;

        let sent = transport.sent();
        assert_eq!(sent.len(), 2, "one logs batch, one traces batch");
        let traces =
            sent.iter().find(|r| r.url.ends_with("/traces/otlp/v0")).expect("traces request");
        assert_eq!(
            traces.url, "http://c/observability/client/traces/otlp/v0",
            "derived from logs endpoint"
        );
        let decoded =
            ExportTraceServiceRequest::decode(&gunzip(&traces.body)[..]).expect("valid OTLP");
        let otlp_span = &decoded.resource_spans[0].scope_spans[0].spans[0];
        assert_eq!(otlp_span.name, "lk.connect");
        assert_eq!(otlp_span.kind, span::SpanKind::Client as i32);
        assert_eq!(hex(&otlp_span.trace_id), telemetry.trace_id());
        assert_eq!(otlp_span.span_id, connect.to_be_bytes().to_vec());
        assert!(otlp_span.parent_span_id.is_empty());
        assert!(otlp_span.end_time_unix_nano >= otlp_span.start_time_unix_nano);
        assert_eq!(otlp_span.events[0].name, "ws_open");
        assert_eq!(
            otlp_span.status.as_ref().map(|s| s.code),
            Some(status::StatusCode::Error as i32)
        );
        assert_eq!(otlp_span.status.as_ref().map(|s| s.message.as_str()), Some("timeout"));
        let attr = |key: &str| {
            otlp_span
                .attributes
                .iter()
                .find(|kv| kv.key == key)
                .and_then(|kv| kv.value.as_ref()?.value.clone())
        };
        assert_eq!(attr("lk.outcome"), Some(Value::StringValue("error".into())));
        assert_eq!(attr("error.type"), Some(Value::StringValue("timeout".into())));
        assert_eq!(attr("lk.connect.attempt"), Some(Value::IntValue(1)));

        let logs = sent.iter().find(|r| r.url.ends_with("/logs/otlp/v0")).expect("logs request");
        let record = &records(logs)[0];
        assert_eq!(hex(&record.trace_id), telemetry.trace_id(), "every record carries the trace");
        assert_eq!(
            record.span_id,
            connect.to_be_bytes().to_vec(),
            "and the span it was emitted in"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cancelled_spans_keep_status_unset() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        let publish = telemetry.begin_span("lk.publish", SpanKind::Internal, None);
        telemetry.end_span(publish, SpanOutcome::Cancelled, None, vec![]);
        telemetry.flush().await;
        let sent = transport.sent();
        assert_eq!(sent[0].url, "http://collector/v1/traces");
        let decoded =
            ExportTraceServiceRequest::decode(&gunzip(&sent[0].body)[..]).expect("valid OTLP");
        let otlp_span = &decoded.resource_spans[0].scope_spans[0].spans[0];
        assert_eq!(
            otlp_span.status.as_ref().map(|s| s.code),
            Some(status::StatusCode::Unset as i32)
        );
        let outcome = otlp_span
            .attributes
            .iter()
            .find(|kv| kv.key == "lk.outcome")
            .and_then(|kv| kv.value.as_ref()?.value.clone());
        assert_eq!(outcome, Some(Value::StringValue("cancelled".into())));
    }

    pub(crate) fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[tokio::test(start_paused = true)]
    async fn entering_background_flushes_immediately() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.set_device_state(DeviceState {
            app_state: AppState::Background,
            ..DeviceState::default()
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        let sent = transport.sent();
        assert_eq!(sent.len(), 1, "flushed on the state change, not on the tick");
        assert!(event_names(&sent[0]).contains(&"lk.ping".to_owned()));
    }

    /// Finding 12: more finished spans than one batch holds all reach the cache (and the wire)
    /// at shutdown; none stay behind in memory uncounted.
    #[tokio::test(start_paused = true)]
    async fn shutdown_drains_every_finished_span_batch() {
        let transport = FakeTransport::scripted([]);
        let mut config = test_config();
        config.max_batch_size = 2;
        let telemetry = start(config, transport.clone());
        tokio::time::sleep(Duration::from_millis(10)).await; // the start-up tick is behind us
        let session = telemetry.begin_scope();
        for _ in 0..5 {
            session.start(SpanName::Publish, None).end(SpanOutcome::Ok, None);
        }
        telemetry.shutdown().await;
        assert_eq!(exported_spans(&transport).len(), 5, "three batches: 2 + 2 + 1");
        assert_eq!(telemetry.stats().dropped, 0);
    }
}
