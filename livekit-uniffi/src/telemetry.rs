//! Client telemetry core from the [`livekit-telemetry`] crate.
//!
//! FFI clients construct one [`Telemetry`] per pipeline, then `emit` from any thread, push
//! `getStats()` readings with `record_stats` and `DeviceState` changes as the OS reports them.
//! The exporter runs on the global runtime; `shutdown` flushes within `export_timeout_ms`.
//!
//! Transport: pass a host-implemented `TelemetryTransport` (a URLSession/OkHttp/dart:io POST,
//! or a data channel), or pass `None` to ride the HTTP client the host registered with
//! `livekit-net` (`set_http_client`) for signaling — one registration serves both.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use livekit_telemetry::{
    Attribute, AttributeValue, DeviceEvent, DeviceState, ExportError, ExportRequest, LogRecord,
    NetTransport, RoomIdentity, RtcStatsSample, SpanName, SpanOutcome, SpanStep, SpanTrack,
    TelemetryConfig, TelemetryEvent, TelemetryStats, TelemetryTransport, TraceContext,
};
use tokio::sync::{mpsc, oneshot};

/// Why a [`Telemetry`] pipeline could not be created.
#[derive(uniffi::Error, thiserror::Error, Debug)]
#[uniffi(flat_error)]
pub enum TelemetryError {
    /// No transport was passed and no HTTP client is registered with `livekit-net`.
    #[error("no telemetry transport: pass one, or register an HTTP client with livekit-net first")]
    NoTransport,
}

/// Telemetry pipeline: buffer, batch, cache and export events as OTLP.
#[derive(uniffi::Object)]
pub struct Telemetry(livekit_telemetry::Telemetry);

#[uniffi::export(async_runtime = "tokio")]
impl Telemetry {
    /// `transport = None` uses the HTTP client registered with `livekit-net`, if any.
    #[uniffi::constructor]
    pub fn new(
        config: TelemetryConfig,
        transport: Option<Arc<dyn TelemetryTransport>>,
    ) -> Result<Arc<Self>, TelemetryError> {
        let transport: Arc<dyn TelemetryTransport> = match transport {
            Some(transport) => transport,
            None => Arc::new(NetTransport::from_registry().ok_or(TelemetryError::NoTransport)?),
        };
        let (telemetry, exporter) = livekit_telemetry::Telemetry::new(config, transport);
        crate::runtime::runtime().spawn(exporter.run());
        Ok(Arc::new(Self(telemetry)))
    }

    /// Like [`Telemetry::new`], but exports through a [`TelemetryExportQueue`] the host drains
    /// from its own thread. For bindings whose callbacks cannot be invoked from Rust threads
    /// (uniffi-dart today).
    #[uniffi::constructor]
    pub fn new_pulled(config: TelemetryConfig, queue: Arc<TelemetryExportQueue>) -> Arc<Self> {
        let (telemetry, exporter) = livekit_telemetry::Telemetry::new(config, queue);
        crate::runtime::runtime().spawn(exporter.run());
        Arc::new(Self(telemetry))
    }

    /// Queue an event for export. Never blocks; drops the oldest event when the queue is full.
    pub fn emit(&self, event: TelemetryEvent) {
        self.0.emit(event);
    }

    /// A consumer-defined event, exported as `custom.<name>`; attributes keep their own namespace.
    pub fn emit_custom(&self, name: String, attributes: Vec<Attribute>) {
        self.0.emit_custom(&name, attributes);
    }

    /// A captured log line; the core applies the per-source floor and builds the record.
    pub fn log(&self, record: LogRecord) {
        self.0.log(record);
    }

    /// Audio route, interruption, denied permission: a process-level record built by the core.
    pub fn device_event(&self, event: DeviceEvent) {
        self.0.device_event(event);
    }

    /// Cloud rule: server URL → observability endpoint, room token → bearer header. No-op when
    /// the config names an explicit endpoint.
    pub fn set_server(&self, url: String, token: String) {
        self.0.set_server(&url, &token);
    }

    /// Where to send, once known (first connect: server URL → endpoint, token → headers). Until
    /// then everything waits in the cache; afterwards it uploads.
    pub fn set_destination(&self, endpoint: String, headers: HashMap<String, String>) {
        self.0.set_destination(&endpoint, headers);
    }

    /// A session — one room, one call — with its own trace id and attributes on this pipeline.
    pub fn begin_session(&self) -> Arc<TelemetrySession> {
        Arc::new(TelemetrySession(self.0.begin_session()))
    }

    /// Report the device state (thermal, low power, foreground/background). Emits the matching
    /// `lk.device.*.changed` events and adapts the export cadence.
    pub fn set_device_state(&self, state: DeviceState) {
        self.0.set_device_state(state);
    }

    /// A pipeline-wide attribute (`enduser.id`, `acme.tenant`), attached to every record of every
    /// session unless the record or its session already carries the key; `None` removes it.
    pub fn set_attribute(&self, key: String, value: Option<AttributeValue>) {
        self.0.set_attribute(&key, value);
    }

    /// The session's trace id (32 hex chars) — on every span and record of this pipeline.
    pub fn trace_id(&self) -> String {
        self.0.trace_id()
    }

    /// Push one `getStats()` reading for a track; windowed on device into `lk.rtc.stats.sample`.
    pub fn record_stats(&self, sample: RtcStatsSample) {
        self.0.record_stats(sample);
    }

    /// Export everything queued and wait for the transport.
    pub async fn flush(&self) {
        self.0.flush().await;
    }

    /// Flush, then stop exporting. Bounded by `export_timeout_ms`.
    pub async fn shutdown(&self) {
        self.0.shutdown().await;
    }

    /// Pipeline health: drops by reason, uploads, cached batches.
    pub fn stats(&self) -> TelemetryStats {
        self.0.stats()
    }

    /// The stats as one line for a debug console.
    pub fn diagnostics(&self) -> String {
        self.0.stats().to_string()
    }
}

/// One room's session on the process pipeline: what its spans, stats and events are filed
/// under. Obtained from [`Telemetry::begin_session`].
#[derive(uniffi::Object)]
pub struct TelemetrySession(livekit_telemetry::Session);

#[uniffi::export]
impl TelemetrySession {
    /// The session's trace id as 32 hex characters.
    pub fn trace_id(&self) -> String {
        self.0.trace_id()
    }

    pub fn emit(&self, event: TelemetryEvent) {
        self.0.emit(event);
    }

    pub fn emit_custom(&self, name: String, attributes: Vec<Attribute>) {
        self.0.emit_custom(&name, attributes);
    }

    /// Attach an attribute to every record of this session from now on; `None` removes it.
    pub fn set_attribute(&self, key: String, value: Option<AttributeValue>) {
        self.0.set_attribute(&key, value);
    }

    pub fn record_stats(&self, sample: RtcStatsSample) {
        self.0.record_stats(sample);
    }

    /// Start a typed span in this session's trace, stamped now; `parent` nests it.
    pub fn start(&self, name: SpanName, parent: Option<Arc<TelemetrySpan>>) -> Arc<TelemetrySpan> {
        Arc::new(TelemetrySpan(self.0.start(name, parent.map(|p| p.0.clone()))))
    }

    /// The room and local participant, on every record of this session from now on.
    pub fn set_room(&self, room: RoomIdentity) {
        self.0.set_room(room);
    }
}

/// One export the host has to perform on behalf of a pulled pipeline.
#[derive(uniffi::Record)]
pub struct PendingExport {
    pub id: u64,
    pub request: ExportRequest,
}

struct Pending {
    export: PendingExport,
    done: oneshot::Sender<Result<(), ExportError>>,
}

/// Pull-side transport: Rust never calls into the host. The exporter queues each request; the
/// host awaits [`next`](Self::next) (a Rust future — those cross every binding), performs the
/// HTTP call on its own thread, and reports the outcome with [`complete`](Self::complete),
/// which unblocks the exporter's retry/drop/go-silent logic exactly as a direct transport would.
///
/// Exists because uniffi-dart's foreign-trait callbacks are isolate-bound (`Pointer.fromFunction`)
/// and abort the VM when invoked from a tokio thread; Swift and Kotlin callbacks are thread-agnostic
/// and use [`TelemetryTransport`] directly.
/// One attempt at an SDK operation, owned by the core. Every call is synchronous and stamps the
/// clock inside, so the only skew is the FFI call; `describe()` is the console line on every
/// platform. Detached (no session) it still times and describes itself.
#[derive(uniffi::Object)]
pub struct TelemetrySpan(Arc<livekit_telemetry::Span>);

#[uniffi::export]
impl TelemetrySpan {
    /// Timings and a description only; nothing is exported.
    #[uniffi::constructor]
    pub fn detached(name: SpanName) -> Arc<Self> {
        Arc::new(Self(livekit_telemetry::Span::detached(name)))
    }

    pub fn label(&self) -> String {
        self.0.label()
    }

    /// A checkpoint, stamped now.
    pub fn step(&self, step: SpanStep) {
        self.0.step(step);
    }

    /// The open bag; replaces an existing key.
    pub fn set_attribute(&self, key: String, value: AttributeValue) {
        self.0.set_attribute(key, value);
    }

    pub fn set_track(&self, track: SpanTrack) {
        self.0.set_track(track);
    }

    /// End once; `error` becomes `error.type` and the status message.
    pub fn end(&self, outcome: SpanOutcome, error: Option<String>) {
        self.0.end(outcome, error);
    }

    pub fn fail(&self, error: String) {
        self.0.fail(error);
    }

    pub fn cancel(&self) {
        self.0.cancel();
    }

    pub fn is_ended(&self) -> bool {
        self.0.is_ended()
    }

    pub fn outcome(&self) -> Option<SpanOutcome> {
        self.0.outcome()
    }

    /// `None` for a detached span.
    pub fn context(&self) -> Option<TraceContext> {
        self.0.context()
    }

    /// Seconds to the end, or to the last step while running.
    pub fn total_secs(&self) -> f64 {
        self.0.total_secs()
    }

    /// `lk.connect: ws_open +1.49s, …, total 1.83s, ok`
    pub fn describe(&self) -> String {
        self.0.describe()
    }
}

/// A span's wire name (`lk.connect`, …, or the custom name), for the platform's own logging.
#[uniffi::export]
pub fn span_label(name: SpanName) -> String {
    name.label().to_owned()
}

#[derive(uniffi::Object)]
pub struct TelemetryExportQueue {
    tx: mpsc::UnboundedSender<Pending>,
    rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<Pending>>,
    inflight: Mutex<HashMap<u64, oneshot::Sender<Result<(), ExportError>>>>,
    seq: AtomicU64,
}

#[uniffi::export(async_runtime = "tokio")]
impl TelemetryExportQueue {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            tx,
            rx: tokio::sync::Mutex::new(rx),
            inflight: Mutex::new(HashMap::new()),
            seq: AtomicU64::new(0),
        })
    }

    /// The next request to perform. Resolves when one is queued; `None` once the pipeline is gone.
    pub async fn next(&self) -> Option<PendingExport> {
        let pending = self.rx.lock().await.recv().await?;
        self.inflight
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pending.export.id, pending.done);
        Some(pending.export)
    }

    /// Report how the request with `id` went: `None` = accepted by the collector.
    pub fn complete(&self, id: u64, error: Option<ExportError>) {
        let done = self.inflight.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        if let Some(done) = done {
            let _ = done.send(error.map_or(Ok(()), Err));
        }
    }
}

#[async_trait::async_trait]
impl TelemetryTransport for TelemetryExportQueue {
    async fn send(&self, request: ExportRequest) -> Result<(), ExportError> {
        let id = self.seq.fetch_add(1, Ordering::Relaxed);
        let (done, wait) = oneshot::channel();
        let pending = Pending { export: PendingExport { id, request }, done };
        if self.tx.send(pending).is_err() {
            return Err(ExportError::Retryable {
                message: "export queue closed".into(),
                retry_after_ms: None,
            });
        }
        wait.await.unwrap_or(Err(ExportError::Retryable {
            message: "host dropped the export".into(),
            retry_after_ms: None,
        }))
    }
}
