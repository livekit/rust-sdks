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
