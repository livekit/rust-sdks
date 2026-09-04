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
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use crate::span::SpanKind;
use crate::{
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

/// Pipeline configuration.
///
/// Defaults follow OTel's `BatchLogRecordProcessor` (1 s delay, 2048 queue, 512 batch);
/// production tuning (e.g. a 15 s cadence) is a config change, not a code change.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Full OTLP/HTTP logs URL: `http://localhost:4318/v1/logs` locally,
    /// `https://<domain>/observability/logs/otlp/v0` for LiveKit Cloud. `None` starts the
    /// pipeline without a destination — it buffers (and caches) until
    /// [`Telemetry::set_destination`], typically at the first connect, when the server URL and
    /// the token are known.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub endpoint: Option<String>,
    /// OTLP/HTTP traces URL. `None` derives it from `endpoint` by replacing the last `logs`
    /// path segment with `traces` (works for both layouts above).
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub traces_endpoint: Option<String>,
    /// Extra request headers, e.g. `Authorization: Bearer <token>`.
    pub headers: HashMap<String, String>,
    /// Resource attributes describing the emitter (`service.name`, `os.name`,
    /// `device.model.identifier`, `session.id`, …). `telemetry.sdk.*` are filled in by the core.
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
    #[cfg_attr(feature = "uniffi", uniffi(default = 1000))]
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
    /// RTC stats window: readings pushed with [`Telemetry::record_stats`] are summarised into one
    /// `lk.rtc.stats.sample` per track and direction every window (stretched like the cadence).
    #[cfg_attr(feature = "uniffi", uniffi(default = 15000))]
    pub stats_window_ms: u64,
    /// Flood guard for discrete events: beyond this many `emit`s per 10 minutes the rest are
    /// dropped and counted as `rate_limited`. RTC windows and self-telemetry are exempt; 0 = off.
    #[cfg_attr(feature = "uniffi", uniffi(default = 300))]
    pub max_events_per_10min: u32,
    /// Cached batches uploaded per tick while a session may be live — bounds how fast a backlog
    /// (offline period, previous launch) replays next to a call: 4 × ~20 KB gzipped per 15 s is
    /// ~40 kbps. `shutdown` drains without the budget.
    #[cfg_attr(feature = "uniffi", uniffi(default = 4))]
    pub max_batches_per_upload: u32,
    /// Export as soon as the queue holds about this many bytes, without waiting for the tick
    /// (design doc: "flush every 15 s or at 256 KB").
    #[cfg_attr(feature = "uniffi", uniffi(default = 262144))]
    pub flush_threshold_bytes: u64,
    /// Cap on one request's payload before compression (design doc: "single POST ≤ 1 MB").
    #[cfg_attr(feature = "uniffi", uniffi(default = 1048576))]
    pub max_batch_bytes: u64,
    /// Lowest severity a plain log record (an event with no name) needs to leave the device.
    /// Events are not subject to it. Design doc: warn.
    // No uniffi default: enum defaults are not supported by the Swift generator (uniffi 0.31).
    pub log_severity: Severity,
}

impl TelemetryConfig {
    /// Defaults for the given endpoint; in-memory cache.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: Some(endpoint.into()),
            traces_endpoint: None,
            headers: HashMap::new(),
            resource: Vec::new(),
            sdk: None,
            storage_dir: None,
            max_cache_bytes: 4 * 1024 * 1024,
            flush_interval_ms: 1000,
            max_queue_size: 2048,
            max_batch_size: 512,
            export_timeout_ms: 10_000,
            stats_window_ms: 15_000,
            max_events_per_10min: 300,
            max_batches_per_upload: 4,
            flush_threshold_bytes: 256 * 1024,
            max_batch_bytes: 1024 * 1024,
            log_severity: Severity::Warn,
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
/// #     async fn send(&self, _: ExportRequest) -> Result<(), ExportError> { Ok(()) }
/// # }
/// # #[tokio::main(flavor = "current_thread")] async fn main() {
/// let (telemetry, exporter) =
///     Telemetry::new(TelemetryConfig::new("http://localhost:4318/v1/logs"), Arc::new(Discard));
/// tokio::spawn(exporter.run());
///
/// telemetry.emit(TelemetryEvent::new("lk.ping"));
/// telemetry.shutdown().await;
/// # }
/// ```
#[derive(Clone)]
pub struct Telemetry {
    store: Arc<Store>,
    config: Arc<TelemetryConfig>,
    cache: Arc<dyn BatchCache>,
    counters: Arc<Counters>,
    device: Arc<Mutex<Option<DeviceState>>>,
    windows: Arc<Mutex<StatsWindows>>,
    guard: Arc<Mutex<FloodGuard>>,
    spans: Arc<Mutex<Spans>>,
    /// The pipeline's own session: whatever is emitted outside a room session.
    process: Arc<ScopeState>,
    /// Attributes attached to every record of every session.
    global: Arc<Mutex<Vec<Attribute>>>,
    destination: Arc<Mutex<Option<Destination>>>,
    status: Arc<Mutex<TelemetryStatus>>,
    commands: mpsc::UnboundedSender<Command>,
}

/// Where batches are sent: the OTLP endpoints and the request headers (auth).
#[derive(Debug, Clone)]
pub(crate) struct Destination {
    pub logs: String,
    pub traces: String,
    pub headers: HashMap<String, String>,
}

impl Destination {
    fn new(logs: &str, traces: Option<String>, headers: HashMap<String, String>) -> Self {
        Self {
            logs: logs.to_owned(),
            traces: traces.unwrap_or_else(|| derive_traces_endpoint(logs)),
            headers,
        }
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
        let cache: Arc<dyn BatchCache> = match config.storage_dir.as_deref() {
            Some(dir) => match FileCache::open(dir, config.max_cache_bytes) {
                Ok(cache) => Arc::new(cache),
                Err(err) => {
                    log::warn!("cannot use storage dir {dir}: {err}; caching in memory");
                    Arc::new(MemoryCache::new(config.max_cache_bytes))
                }
            },
            None => Arc::new(MemoryCache::new(config.max_cache_bytes)),
        };
        Self::with_cache(config, transport, cache)
    }

    /// Build the pipeline around a caller-provided [`BatchCache`] (`storage_dir` is ignored).
    pub fn with_cache(
        mut config: TelemetryConfig,
        transport: Arc<dyn TelemetryTransport>,
        cache: Arc<dyn BatchCache>,
    ) -> (Self, Exporter) {
        add_sdk_resource(&mut config.resource, config.sdk.as_ref());
        let destination = Arc::new(Mutex::new(config.endpoint.as_deref().map(|endpoint| {
            Destination::new(endpoint, config.traces_endpoint.clone(), config.headers.clone())
        })));
        let config = Arc::new(config);
        // One pipeline per process; sessions (rooms) carry their own trace ids. This is the
        // pipeline's own session, for everything emitted outside a room.
        let process = ScopeState::new();
        let global = Arc::new(Mutex::new(Vec::new()));
        let counters = Arc::new(Counters::default());
        let status = Arc::new(Mutex::new(TelemetryStatus::Ok));
        let store = Arc::new(Store::new(
            config.max_queue_size.max(1) as usize,
            usize::try_from(config.flush_threshold_bytes.max(1)).unwrap_or(usize::MAX),
            counters.clone(),
        ));
        let (commands, receiver) = mpsc::unbounded_channel();
        let windows = Arc::new(Mutex::new(StatsWindows::default()));
        let guard = Arc::new(Mutex::new(FloodGuard::new(config.max_events_per_10min)));
        let spans = Arc::new(Mutex::new(Spans::new(config.max_queue_size.max(1) as usize)));
        let device = Arc::new(Mutex::new(None));
        let exporter = Exporter::new(
            store.clone(),
            transport,
            config.clone(),
            cache.clone(),
            counters.clone(),
            windows.clone(),
            spans.clone(),
            process.clone(),
            global.clone(),
            destination.clone(),
            receiver,
            device.clone(),
            status.clone(),
        );
        let telemetry = Self {
            store,
            config,
            cache,
            counters,
            device,
            windows,
            guard,
            spans,
            process,
            global,
            destination,
            status,
            commands,
        };
        (telemetry, exporter)
    }

    /// The Cloud rule, in one place for every SDK: the room's server URL names the observability
    /// endpoint and the room token authorizes it. An explicit `TelemetryConfig::endpoint` (a
    /// collector of your own) wins and makes this a no-op.
    pub fn set_server(&self, url: &str, token: &str) {
        if self.config.endpoint.is_some() {
            return;
        }
        match observability_endpoint(url) {
            Some(endpoint) => {
                let mut headers = HashMap::new();
                headers.insert("Authorization".to_owned(), format!("Bearer {token}"));
                self.set_destination(&endpoint, headers);
            }
            None => log::warn!("server url has no host; uploads stay cached"),
        }
    }

    /// Where to send, once known (the first connect: server URL → endpoint, token → headers).
    /// Everything cached so far starts uploading. Calling it again (new token, new server)
    /// replaces the destination for the batches that follow.
    pub fn set_destination(&self, endpoint: &str, headers: HashMap<String, String>) {
        *self.destination.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(Destination::new(endpoint, None, headers));
        let _ = self.commands.send(Command::Overflow);
    }

    /// Start a session — one room, one call — with its own trace id and attributes on this
    /// pipeline. Sessions do not need ending: a room's last record is simply its last.
    pub fn begin_scope(&self) -> Scope {
        Scope { telemetry: self.clone(), state: ScopeState::new() }
    }

    /// A span in the pipeline's own trace: app-defined work outside any room, or the SDK before a
    /// room exists. Stamped now; `parent` nests it.
    pub fn start(&self, name: SpanName, parent: Option<Arc<Span>>) -> Arc<Span> {
        let parent = parent.and_then(|p| p.context()).map(|c| c.span_id);
        Span::bound(name, parent, self.clone(), &self.process)
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
        self.emit_in(event.into_event(), &self.process);
    }

    /// A captured log line. WebRTC only counts at error; the SDK and the core at the configured
    /// floor; the core's own telemetry module never (a rejected batch that produced a record that
    /// produced a batch would never end).
    pub fn log(&self, record: LogRecord) {
        let floor = match record.source {
            LogSource::WebRtc => self.config.log_severity.max(Severity::Error),
            _ => self.config.log_severity,
        };
        if record.severity < floor {
            return;
        }
        if record.source == LogSource::Ffi
            && record.logger.as_deref().is_some_and(|l| l.starts_with("livekit_telemetry"))
        {
            return;
        }
        self.emit(record.into());
    }

    pub fn emit(&self, event: TelemetryEvent) {
        // A record emitted inside a room's span belongs to that room's session; anything else
        // is the process's own.
        let session = event
            .span_id
            .and_then(|id| self.spans.lock().unwrap_or_else(|e| e.into_inner()).scope_of(id))
            .unwrap_or_else(|| self.process.clone());
        self.emit_in(event, &session);
    }

    pub(crate) fn emit_in(&self, mut event: TelemetryEvent, session: &Arc<ScopeState>) {
        if event.name.is_empty() && event.severity < self.config.log_severity {
            return;
        }
        if !self.guard.lock().unwrap_or_else(|e| e.into_inner()).admit() {
            Counters::add(&self.counters.rate_limited, 1);
            return;
        }
        if event.timestamp_ns.is_none() {
            event.timestamp_ns = Some(now_unix_nanos());
        }
        if self.store.push(Queued { event, session: session.clone() }) {
            let _ = self.commands.send(Command::Overflow);
        }
    }

    /// Queue a consumer-defined event, exported as `custom.<name>` (see
    /// [`TelemetryEvent::custom`]): the stringly-typed escape hatch next to the `lk.*`
    /// catalogue. Same flood guard, same pipeline.
    pub fn emit_custom(&self, name: &str, attributes: Vec<Attribute>) {
        self.emit(TelemetryEvent::custom(name, attributes));
    }

    /// Set a pipeline-wide attribute (a consumer's `enduser.id`, an `acme.tenant`), attached to
    /// every record of every session from now on unless the record — or its session — already
    /// carries the key. `None` removes it. Scope-level identity goes through
    /// [`Scope::set_attribute`].
    pub fn set_attribute(&self, key: &str, value: Option<AttributeValue>) {
        let mut global = self.global.lock().unwrap_or_else(|e| e.into_inner());
        global.retain(|a| a.key != key);
        if let Some(value) = value {
            global.push(Attribute::new(key, value));
        }
    }

    /// The session's trace id as 32 hex characters — what every span and log record of this
    /// pipeline carries. Print it (`lkt_…`) so support can find the session.
    pub fn trace_id(&self) -> String {
        self.process.hex()
    }

    /// Open a span: one attempt at an operation (`lk.connect`, `lk.publish`, …). Returns the
    /// handle to record checkpoints and to end it with; `parent` nests it under another open span.
    #[cfg(test)]
    pub(crate) fn begin_span(&self, name: &str, kind: SpanKind, parent: Option<u64>) -> u64 {
        self.begin_span_in(name, kind, parent, &self.process)
    }

    pub(crate) fn begin_span_in(
        &self,
        name: &str,
        kind: SpanKind,
        parent: Option<u64>,
        session: &Arc<ScopeState>,
    ) -> u64 {
        self.spans.lock().unwrap_or_else(|e| e.into_inner()).begin_in(
            name,
            kind,
            parent,
            session.clone(),
        )
    }

    /// Record a checkpoint inside an open span (`ws_open`, `join_recv`, …), stamped now.
    pub(crate) fn add_span_event(&self, span: u64, name: &str, attributes: Vec<Attribute>) {
        self.spans.lock().unwrap_or_else(|e| e.into_inner()).add_event(span, name, attributes);
    }

    /// End a span with its outcome; `error_type` becomes `error.type` and the status message.
    /// The span is exported with the next batch. Ending twice, or an unknown handle, is a no-op.
    pub(crate) fn end_span(
        &self,
        span: u64,
        outcome: SpanOutcome,
        error_type: Option<String>,
        attributes: Vec<Attribute>,
    ) {
        self.spans
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .end(span, outcome, error_type, attributes);
    }

    /// Push one `getStats()` reading. Readings are windowed on device into `lk.rtc.stats.sample`
    /// events (see `stats_window_ms`); they never count against the flood guard.
    pub fn record_stats(&self, sample: RtcStatsSample) {
        self.record_stats_in(sample, &self.process);
    }

    pub(crate) fn record_stats_in(&self, sample: RtcStatsSample, session: &Arc<ScopeState>) {
        self.windows.lock().unwrap_or_else(|e| e.into_inner()).record_in(sample, session);
    }

    /// Tell the pipeline what the device looks like. Emits the `lk.device.*.changed` events for
    /// whatever differs from the last state (everything, the first time) and re-tunes the
    /// pipeline: pressure stretches the cadence up to 4× ([`DeviceState::cadence_factor`]), a
    /// constrained network or a nearly empty battery holds uploads
    /// ([`DeviceState::holds_uploads`]), and entering the background flushes once right away.
    pub fn set_device_state(&self, state: DeviceState) {
        let mut previous = self.device.lock().unwrap_or_else(|e| e.into_inner());
        for event in state.change_events(previous.as_ref()) {
            self.emit(event);
        }
        *previous = Some(state);
        let _ = self.commands.send(Command::DeviceState(state));
    }

    /// Cache everything queued and upload what the transport accepts right now.
    pub async fn flush(&self) {
        self.command(Command::Flush).await;
    }

    /// Flush, then stop the exporter. Bounded by `export_timeout_ms`; events emitted afterwards
    /// are never exported. Queued events reach the cache before the network is tried, so with
    /// a [`FileCache`] nothing is lost if the process dies mid-way.
    pub async fn shutdown(&self) {
        let bound = Duration::from_millis(self.config.export_timeout_ms.max(1));
        let _ = timeout(bound, self.command(Command::Shutdown)).await;
    }

    /// Pipeline health: drops by reason, uploads, cached batches. The same numbers ride to the
    /// backend as `lk.telemetry.report` events whenever something went wrong.
    pub fn stats(&self) -> TelemetryStats {
        TelemetryStats::new(
            self.counters.snapshot(),
            self.cache.pending().len() as u64,
            *self.status.lock().unwrap_or_else(|e| e.into_inner()),
        )
    }

    async fn command(&self, make: impl FnOnce(oneshot::Sender<()>) -> Command) {
        let (done, wait) = oneshot::channel();
        if self.commands.send(make(done)).is_ok() {
            let _ = wait.await;
        }
    }
}

/// `…/logs…` → `…/traces…`: covers `/v1/logs` and `/observability/logs/otlp/v0` alike.
fn derive_traces_endpoint(logs_endpoint: &str) -> String {
    match logs_endpoint.rsplit_once("logs") {
        Some((before, after)) => format!("{before}traces{after}"),
        None => logs_endpoint.to_owned(),
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
    fn attributes(&self) -> Vec<Attribute> {
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

/// `wss://x.livekit.cloud/rtc?…` → `https://x.livekit.cloud/observability/logs/otlp/v0`;
/// `ws://`/`http://` stay plain http (dev servers). Host and port only; no path, query or userinfo.
pub(crate) fn observability_endpoint(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    if host.is_empty() {
        return None;
    }
    let scheme = match scheme {
        "ws" | "http" => "http",
        _ => "https",
    };
    Some(format!("{scheme}://{host}/observability/logs/otlp/v0"))
}

#[cfg(test)]
mod tests {
    use crate::span::SpanKind;
    use crate::{RoomIdentity, SpanName, SpanStep};
    use std::{collections::VecDeque, fs, path::Path, sync::Mutex};

    #[test]
    fn server_url_names_the_observability_endpoint() {
        let ep = observability_endpoint;
        assert_eq!(
            ep("wss://x.livekit.cloud").unwrap(),
            "https://x.livekit.cloud/observability/logs/otlp/v0"
        );
        assert_eq!(
            ep("wss://x.livekit.cloud/rtc?access_token=t#f").unwrap(),
            "https://x.livekit.cloud/observability/logs/otlp/v0"
        );
        assert_eq!(
            ep("ws://192.168.99.24:7880").unwrap(),
            "http://192.168.99.24:7880/observability/logs/otlp/v0"
        );
        assert_eq!(ep("https://u:p@host").unwrap(), "https://host/observability/logs/otlp/v0");
        assert_eq!(ep("nonsense"), None);
        assert_eq!(ep("wss:///rtc"), None);
    }

    #[tokio::test(start_paused = true)]
    async fn typed_spans_hold_uploads_while_connecting_and_export_when_ended() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        let session = telemetry.begin_scope();
        let span = session.start(SpanName::Reconnect { reason: "ws closed".into() }, None);
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert!(transport.sent().is_empty(), "an open reconnect holds uploads");
        span.step(SpanStep::Attempt { number: 1, full: false });
        span.end(SpanOutcome::Ok, None);
        assert!(span.context().is_some_and(|c| c.trace_id == session.trace_id()));
        telemetry.flush().await;
        assert!(transport.sent().iter().any(|r| r.url.contains("traces")), "the span is exported");
    }

    #[tokio::test(start_paused = true)]
    async fn room_identity_and_resource_are_typed() {
        let transport = FakeTransport::scripted([]);
        let mut config = TelemetryConfig::new("http://collector/v1/logs");
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
        let logs = records(&sent[0]);
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
            message: "boom".into(),
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
        AppState, ExportError, ExportRequest, SpanOutcome, StreamDirection, ThermalState,
        TrackKind,
    };

    #[derive(Default)]
    struct FakeTransport {
        requests: Mutex<Vec<ExportRequest>>,
        script: Mutex<VecDeque<Result<(), ExportError>>>,
    }

    impl FakeTransport {
        fn scripted(results: impl IntoIterator<Item = Result<(), ExportError>>) -> Arc<Self> {
            Arc::new(Self {
                script: Mutex::new(results.into_iter().collect()),
                ..Default::default()
            })
        }
        fn sent(&self) -> Vec<ExportRequest> {
            self.requests.lock().expect("lock").clone()
        }
    }

    #[async_trait::async_trait]
    impl TelemetryTransport for FakeTransport {
        async fn send(&self, request: ExportRequest) -> Result<(), ExportError> {
            self.requests.lock().expect("lock").push(request);
            self.script.lock().expect("lock").pop_front().unwrap_or(Ok(()))
        }
    }

    fn gunzip(body: &[u8]) -> Vec<u8> {
        use std::io::Read;
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(body).read_to_end(&mut out).expect("gzip body");
        out
    }

    fn offline() -> Result<(), ExportError> {
        Err(ExportError::Retryable { message: "offline".into(), retry_after_ms: None })
    }

    fn offline_forever() -> impl Iterator<Item = Result<(), ExportError>> {
        std::iter::repeat_with(offline).take(64)
    }

    fn pipeline(transport: Arc<FakeTransport>) -> Telemetry {
        start(TelemetryConfig::new("http://collector/v1/logs"), transport)
    }

    fn persisted_pipeline(transport: Arc<FakeTransport>, dir: &Path) -> Telemetry {
        let mut config = TelemetryConfig::new("http://collector/v1/logs");
        config.storage_dir = Some(dir.to_string_lossy().into_owned());
        start(config, transport)
    }

    fn start(config: TelemetryConfig, transport: Arc<FakeTransport>) -> Telemetry {
        let (telemetry, exporter) = Telemetry::new(config, transport);
        tokio::spawn(exporter.run());
        telemetry
    }

    fn files_in(dir: &Path) -> usize {
        fs::read_dir(dir).map(|d| d.count()).unwrap_or(0)
    }

    fn records(request: &ExportRequest) -> Vec<LogRecord> {
        let decoded =
            ExportLogsServiceRequest::decode(&gunzip(&request.body)[..]).expect("valid OTLP");
        decoded.resource_logs[0].scope_logs[0].log_records.clone()
    }

    fn event_names(request: &ExportRequest) -> Vec<String> {
        records(request).iter().map(|r| r.event_name.clone()).collect()
    }

    fn attribute(record: &LogRecord, key: &str) -> Option<Value> {
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
        let transport = FakeTransport::scripted([offline(), offline(), offline()]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 3, "first attempt plus two retries");
        assert_eq!(telemetry.stats().dropped, 0, "kept in the memory cache, not dropped");
        assert_eq!(telemetry.stats().upload_failures, 3);
        assert_eq!(telemetry.stats().cached_batches, 1);

        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 3, "backoff: no upload right away");

        tokio::time::sleep(Duration::from_secs(61)).await;
        assert_eq!(transport.sent().len(), 4, "retried once the backoff elapsed");
        assert_eq!(event_names(&transport.sent()[3]), ["lk.ping"]);
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
        let mut config = TelemetryConfig::new("http://collector/v1/logs");
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
            FakeTransport::scripted([Err(ExportError::Rejected { message: "400".into() })]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;

        assert_eq!(transport.sent().len(), 1);
        assert_eq!(telemetry.stats().dropped_rejected, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn disabled_collector_silences_the_exporter() {
        let transport = FakeTransport::scripted([Err(ExportError::Disabled)]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.shutdown().await;

        assert_eq!(transport.sent().len(), 1);
        assert_eq!(telemetry.stats().dropped_disabled, 2);
    }

    #[tokio::test(start_paused = true)]
    async fn batch_is_written_before_upload_and_replayed_on_next_start() {
        let dir = temp_dir("replay");
        let first_transport = FakeTransport::scripted(offline_forever());
        let first = persisted_pipeline(first_transport.clone(), &dir);
        first.emit(TelemetryEvent::new("lk.ping"));
        first.flush().await;
        assert_eq!(first_transport.sent().len(), 3);
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
    async fn throttling_keeps_cached_batches_and_drops_new_ones() {
        let dir = temp_dir("throttle");
        let throttled =
            Err(ExportError::Retryable { message: "429".into(), retry_after_ms: Some(5_000) });
        let transport = FakeTransport::scripted([throttled]);
        let telemetry = persisted_pipeline(transport.clone(), &dir);
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 1, "no retries on Retry-After");
        assert_eq!(files_in(&dir), 1, "the throttled batch stays cached");
        assert_eq!(telemetry.stats().dropped, 0);

        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert_eq!(telemetry.stats().dropped_throttled, 1, "new batches dropped inside the window");
        assert_eq!(files_in(&dir), 1, "and never written");

        tokio::time::sleep(Duration::from_secs(6)).await;
        assert_eq!(transport.sent().len(), 2, "cached batch uploaded after Retry-After");
        assert_eq!(files_in(&dir), 0);
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
        let (telemetry, exporter) = Telemetry::with_cache(
            TelemetryConfig::new("http://collector/v1/logs"),
            transport,
            cache.clone(),
        );
        tokio::spawn(exporter.run());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        let pending = cache.pending();
        assert_eq!(pending.len(), 1);
        assert!(pending[0].ends_with("-1-l"), "id carries count and signal: {}", pending[0]);
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
        assert_eq!(names.len(), 5, "initial value for every known field (battery unknown)");
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

        tokio::time::sleep(Duration::from_secs(61)).await;
        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 1, "held 60 s: one batch goes out regardless");
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
        let mut config = TelemetryConfig::new("http://collector/v1/logs");
        config.max_batches_per_upload = 2;
        let telemetry = start(config, transport.clone());
        telemetry.set_device_state(DeviceState { network_constrained: true, ..Default::default() });
        for _ in 0..5 {
            telemetry.emit(TelemetryEvent::new("lk.ping"));
            telemetry.flush().await;
        }
        assert!(transport.sent().is_empty(), "Low Data Mode: record, do not upload");
        assert_eq!(telemetry.stats().cached_batches, 5);

        // Back to normal (the change event itself makes a sixth batch).
        telemetry.set_device_state(DeviceState::default());
        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 2, "two batches per tick next to a live call");
        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 4);
        telemetry.shutdown().await;
        assert_eq!(transport.sent().len(), 7, "shutdown drains without the budget (+ the summary)");
    }

    struct Hanging;

    #[async_trait::async_trait]
    impl TelemetryTransport for Hanging {
        async fn send(&self, _: ExportRequest) -> Result<(), ExportError> {
            std::future::pending().await
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_full_queue_flushes_before_the_tick() {
        let transport = FakeTransport::scripted([]);
        let mut config = TelemetryConfig::new("http://collector/v1/logs");
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
        let mut config = TelemetryConfig::new("http://collector/v1/logs");
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
        let (telemetry, exporter) = Telemetry::with_cache(
            TelemetryConfig::new("http://collector/v1/logs"),
            transport.clone(),
            Arc::new(MemoryCache::new(1)),
        );
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
        let (telemetry, exporter) =
            Telemetry::new(TelemetryConfig::new("http://collector/v1/logs"), Arc::new(Hanging));
        tokio::spawn(exporter.run());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await; // 3 attempts × export_timeout, under paused time
        let stats = telemetry.stats();
        assert_eq!(stats.upload_timeouts, 3);
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
        a.set_attribute("lk.room.sid", Some("RM_a".into()));
        b.set_attribute("lk.room.sid", Some("RM_b".into()));
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
        let logs = records(&sent[1]);
        assert_eq!(logs.len(), 4);
        assert_eq!(hex(&logs[0].trace_id), a.trace_id());
        assert_eq!(attribute(&logs[0], "lk.room.sid"), Some(Value::StringValue("RM_a".into())));
        assert_eq!(attribute(&logs[0], "session.id"), Some(Value::StringValue(a.trace_id())));
        assert_eq!(hex(&logs[1].trace_id), b.trace_id());
        assert_eq!(attribute(&logs[1], "lk.room.sid"), Some(Value::StringValue("RM_b".into())));
        assert_eq!(hex(&logs[2].trace_id), a.trace_id(), "resolved through the span");
        assert_eq!(hex(&logs[3].trace_id), telemetry.trace_id(), "device state: process session");
        assert_eq!(attribute(&logs[3], "lk.room.sid"), None);
        assert!(
            logs.iter()
                .all(|r| attribute(r, "acme.tenant") == Some(Value::StringValue("t1".into()))),
            "a pipeline-wide attribute reaches every session"
        );
        let traces = ExportTraceServiceRequest::decode(&gunzip(&sent[0].body)[..]).expect("otlp");
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
        let late = &records(&transport.sent()[2])[0];
        assert_eq!(hex(&late.trace_id), a.trace_id(), "filed under the ended span's session");
        assert_eq!(late.span_id, span_id.to_be_bytes().to_vec());
    }

    #[tokio::test(start_paused = true)]
    async fn uploads_wait_for_a_destination() {
        let transport = FakeTransport::scripted([]);
        let mut config = TelemetryConfig::new("unused");
        config.endpoint = None;
        let telemetry = start(config, transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        tokio::time::sleep(Duration::from_secs(120)).await;
        assert!(transport.sent().is_empty(), "no destination: nothing leaves, no hold cap either");
        assert_eq!(telemetry.stats().cached_batches, 1);

        let mut headers = HashMap::new();
        headers.insert("Authorization".to_owned(), "Bearer t".to_owned());
        telemetry.set_destination("https://x.livekit.cloud/observability/logs/otlp/v0", headers);
        tokio::time::sleep(Duration::from_millis(1)).await;
        let sent = transport.sent();
        assert_eq!(sent.len(), 1, "cached batches ship as soon as the destination is known");
        assert_eq!(sent[0].url, "https://x.livekit.cloud/observability/logs/otlp/v0");
        assert_eq!(sent[0].headers["Authorization"], "Bearer t");
        let span = telemetry.begin_span("lk.publish", SpanKind::Internal, None);
        telemetry.end_span(span, SpanOutcome::Ok, None, Vec::new());
        telemetry.flush().await;
        assert_eq!(transport.sent()[1].url, "https://x.livekit.cloud/observability/traces/otlp/v0");
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
        let mut config = TelemetryConfig::new("http://collector/v1/logs");
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
        let mut config = TelemetryConfig::new("http://collector/v1/logs");
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
        let telemetry =
            start(TelemetryConfig::new("http://c/observability/logs/otlp/v0"), transport.clone());
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
            traces.url, "http://c/observability/traces/otlp/v0",
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

    fn hex(bytes: &[u8]) -> String {
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
}
