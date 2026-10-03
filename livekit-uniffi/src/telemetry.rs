//! Client telemetry core from the [`livekit-telemetry`] crate.
//!
//! What a platform calls, and when:
//!
//! - SDK init: [`telemetry_configure`] (or [`telemetry_configure_pulled`] where foreign callbacks
//!   are thread-bound) with storage and instruments. No endpoint, no headers: destinations come
//!   from the rooms.
//! - Room created: [`telemetry_scope`]; connect: [`TelemetryScope::set_server`] with the server
//!   URL and the participant token, and again with every refreshed token.
//! - Room life: spans ([`TelemetryScope::start`]), subscribe lifecycle, one `getStats()` report
//!   per peer connection ([`TelemetryScope::record_peer_stats`]) every
//!   [`TelemetryScope::stats_poll_interval_ms`], [`TelemetryScope::disconnected`].
//! - App API: [`TelemetryScope::emit_custom`], [`TelemetryScope::set_attribute`] (room-scoped),
//!   [`telemetry_disable`] (process-wide opt-out).
//! - OS signals: [`telemetry_set_device_state`], [`telemetry_device_event`], [`telemetry_log`].
//!
//! Transport: pass a host-implemented `TelemetryTransport`, or `None` to ride the HTTP client
//! the host registered with `livekit-net` (`set_http_client`) for signaling.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use livekit_telemetry::{
    global::{self, TelemetryInstrument},
    Attribute, AttributeValue, DeviceEvent, DeviceState, DisconnectReason, ExportError,
    ExportRequest, ExportResponse, LogRecord, NetTransport, ReconnectReason, RoomIdentity, RtcStat,
    SpanName, SpanOutcome, SpanStep, SpanTrack, StreamDirection, TelemetryConfig, TelemetryEvent,
    TelemetryStats, TelemetryTransport, TraceContext, TrackKind,
};
use tokio::sync::{mpsc, oneshot, watch};

/// Why a [`Telemetry`] pipeline could not be created.
#[derive(uniffi::Error, thiserror::Error, Debug)]
#[uniffi(flat_error)]
pub enum TelemetryError {
    /// No transport was passed and no HTTP client is registered with `livekit-net`.
    #[error("no telemetry transport: pass one, or register an HTTP client with livekit-net first")]
    NoTransport,
}

/// Start the process pipeline; a previous one is drained and replaced. `transport = None` uses
/// the HTTP client registered with `livekit-net`, if any. After [`telemetry_disable`] nothing
/// starts, and whatever `config.storage_dir` still holds is deleted.
#[uniffi::export]
pub fn telemetry_configure(
    config: TelemetryConfig,
    transport: Option<Arc<dyn TelemetryTransport>>,
    instruments: Vec<Arc<dyn TelemetryInstrument>>,
) -> Result<(), TelemetryError> {
    let transport: Arc<dyn TelemetryTransport> = match transport {
        Some(transport) => transport,
        None => Arc::new(NetTransport::from_registry().ok_or(TelemetryError::NoTransport)?),
    };
    install(livekit_telemetry::Telemetry::new(config, transport), instruments);
    Ok(())
}

/// Like [`telemetry_configure`], exporting through a queue the host drains from its own thread.
/// For bindings whose callbacks cannot be invoked from Rust threads (uniffi-dart today): such a
/// host passes no `instruments` — it starts and stops its own around configure and disable, so
/// Rust never calls back into it — and polls the queue with
/// [`try_next`](TelemetryExportQueue::try_next).
#[uniffi::export]
pub fn telemetry_configure_pulled(
    config: TelemetryConfig,
    instruments: Vec<Arc<dyn TelemetryInstrument>>,
) -> Arc<TelemetryExportQueue> {
    let queue = TelemetryExportQueue::new();
    queue.process.store(true, Ordering::SeqCst);
    install(livekit_telemetry::Telemetry::new(config, queue.clone()), instruments);
    queue
}

fn install(
    (telemetry, exporter): (livekit_telemetry::Telemetry, livekit_telemetry::Exporter),
    instruments: Vec<Arc<dyn TelemetryInstrument>>,
) {
    let previous = global::install(telemetry, instruments);
    // Refused after the opt-out (the new pipeline was purged instead, its `storage_dir` with it),
    // or opted out meanwhile (every generation revoked): nothing to run, nothing to drain.
    if global::is_disabled() {
        return;
    }
    crate::runtime::runtime().spawn(exporter.run());
    if let Some(previous) = previous {
        crate::runtime::runtime().spawn(async move { previous.shutdown().await });
    }
}

/// Drain and stop the process pipeline; every call is a no-op afterwards. After
/// [`telemetry_disable`], returns once its purge is complete.
#[uniffi::export(async_runtime = "tokio")]
pub async fn telemetry_shutdown() {
    purged().await;
    global::shutdown().await;
}

/// Opt-out for the rest of the process, in effect when this returns: no scope is handed out,
/// every existing scope and instrument captures nothing, and later configures are refused.
/// Everything not yet sent — queued, open, cached on disk — is then deleted in the background
/// without another upload (awaited by [`telemetry_shutdown`] and [`telemetry_flush`]). Data
/// already sent cannot be recalled.
///
/// The installed instruments' `stop()` runs synchronously on the calling thread, under the
/// lifecycle lock, before this returns: it must not block on the caller's queue and must not
/// call back into configure, disable, flush or shutdown.
#[uniffi::export]
pub fn telemetry_disable() {
    start_purge(&PURGE, global::disable());
}

/// Whether the process opted out with [`telemetry_disable`]: `true` for every thread and isolate
/// from the moment that call revokes collection, before it returns. A cheap read that never
/// panics; platforms with per-isolate state may check it right before each `getStats` or submit.
#[uniffi::export]
pub fn telemetry_is_disabled() -> bool {
    global::is_disabled()
}

/// The opt-out's purge in progress: done once it holds `true`.
type PurgeSlot = Mutex<Option<watch::Receiver<bool>>>;

/// The process-wide opt-out's purge.
static PURGE: PurgeSlot = Mutex::new(None);

/// Run `purge` in the background and record it in `slot`, done only once every purge recorded
/// there before it is done too: a repeated opt-out never lets a waiter skip the first purge.
fn start_purge(slot: &PurgeSlot, purge: impl std::future::Future + Send + 'static) {
    let (done, receiver) = watch::channel(false);
    let previous = slot.lock().unwrap_or_else(|e| e.into_inner()).replace(receiver);
    crate::runtime::runtime().spawn(async move {
        purge.await;
        if let Some(mut previous) = previous {
            let _ = previous.wait_for(|done| *done).await;
        }
        let _ = done.send(true);
    });
}

/// Wait for the opt-out's purge, if one was started.
async fn purged() {
    wait_purge(&PURGE).await;
}

/// Wait for the purge recorded in `slot`, if any.
async fn wait_purge(slot: &PurgeSlot) {
    let receiver = slot.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(mut receiver) = receiver {
        let _ = receiver.wait_for(|done| *done).await;
    }
}

/// Cache everything queued and upload what the network allows. After [`telemetry_disable`],
/// returns once its purge is complete.
#[uniffi::export(async_runtime = "tokio")]
pub async fn telemetry_flush() {
    purged().await;
    global::flush().await;
}

/// A scope — one room, one call — on the process pipeline; `None` while telemetry is off.
#[uniffi::export]
pub fn telemetry_scope() -> Option<Arc<TelemetryScope>> {
    global::scope().map(|scope| Arc::new(TelemetryScope(scope)))
}

/// A process-level event (outside any room).
#[uniffi::export]
pub fn telemetry_emit(event: TelemetryEvent) {
    global::emit(event);
}

/// A captured log line; the core applies the per-source floor and builds the record. For the
/// platform's own and WebRTC's lines only — never a Rust entry received through the log
/// forwarder, which the core has already copied (`log_forward_bootstrap`).
#[uniffi::export]
pub fn telemetry_log(record: LogRecord) {
    global::log(record);
}

/// Audio route, interruption, denied permission: a process-level record built by the core.
#[uniffi::export]
pub fn telemetry_device_event(event: DeviceEvent) {
    global::device_event(event);
}

/// The device's current state; drives the upload cadence and yields change events.
#[uniffi::export]
pub fn telemetry_set_device_state(state: DeviceState) {
    global::set_device_state(state);
}

/// SDK metadata on every record of every scope (not an app API: app attributes are
/// room-scoped, [`TelemetryScope::set_attribute`]); `None` removes it.
#[uniffi::export]
pub fn telemetry_set_attribute(key: String, value: Option<AttributeValue>) {
    global::set_attribute(&key, value);
}

/// Pipeline health (drops by reason, uploads, backlog); `None` while telemetry is off.
#[uniffi::export]
pub fn telemetry_stats() -> Option<TelemetryStats> {
    global::stats()
}

/// The stats as one line for a debug console, or `off`.
#[uniffi::export]
pub fn telemetry_diagnostics() -> String {
    global::diagnostics()
}

/// One Room's telemetry session: its trace, attributes, spans, stats and destination.
#[derive(uniffi::Object)]
pub struct TelemetryScope(livekit_telemetry::Scope);

#[uniffi::export]
impl TelemetryScope {
    /// The session's trace id as 32 hex characters.
    pub fn trace_id(&self) -> String {
        self.0.trace_id()
    }

    /// An SDK event filed under this session.
    pub fn emit(&self, event: TelemetryEvent) {
        self.0.emit(event);
    }

    /// The room's server URL and participant token: at connect and with every refreshed token.
    /// Cheap and idempotent. The core derives the ingest URL (LiveKit Cloud only), reads the
    /// grant and expiry, and uploads this room's records with this room's token.
    pub fn set_server(&self, url: String, token: String) {
        self.0.set_server(&url, &token);
    }

    /// An app event, exported as `custom.<name>` with the room's correlation attributes. Names
    /// and keys up to 128 bytes, values up to 1024, at most 64 attributes, no `lk.*` keys;
    /// anything else is rejected and counted.
    pub fn emit_custom(&self, name: String, attributes: HashMap<String, String>) {
        let attributes = attributes.into_iter().map(|(k, v)| Attribute::new(k, v)).collect();
        self.0.emit_custom(&name, attributes);
    }

    /// An app correlation attribute on every record this room captures from now on; `None`
    /// removes it. Same limits as `emit_custom`, at most 64 per room.
    pub fn set_attribute(&self, key: String, value: Option<String>) {
        self.0.set_attribute(&key, value.map(AttributeValue::Str));
    }

    /// One peer connection's whole `getStats()` report and its tracks (MediaStreamTrack id →
    /// track sid); the core maps every RTP stream to its track.
    pub fn record_peer_stats(
        &self,
        report: Vec<RtcStat>,
        tracks: HashMap<String, String>,
        timestamp_ns: Option<u64>,
    ) {
        self.0.record_peer_stats(report, tracks, timestamp_ns);
    }

    /// When to poll `getStats()` next, in milliseconds; ask after every poll.
    pub fn stats_poll_interval_ms(&self) -> u64 {
        self.0.stats_poll_interval_ms()
    }

    /// Start a typed span in this session's trace, stamped now; `parent` nests it.
    pub fn start(&self, name: SpanName, parent: Option<Arc<TelemetrySpan>>) -> Arc<TelemetrySpan> {
        Arc::new(TelemetrySpan(self.0.start(name, parent.map(|p| p.0.clone()))))
    }

    /// The room and local participant, on every record of this session from now on.
    pub fn set_room(&self, room: RoomIdentity) {
        self.0.set_room(room);
    }

    /// The session ended for good (not a reconnect).
    pub fn disconnected(&self, reason: DisconnectReason) {
        self.0.disconnected(reason);
    }

    /// A log record filed under this session even without an ambient span.
    pub fn log(&self, record: LogRecord) {
        self.0.log(record);
    }

    /// One track's whole `getStats()` report; the core maps it (see `record_stats`).
    pub fn record_stats_report(
        &self,
        track_sid: String,
        kind: TrackKind,
        direction: StreamDirection,
        report: Vec<RtcStat>,
        timestamp_ns: Option<u64>,
    ) {
        self.0.record_stats_report(&track_sid, kind, direction, report, timestamp_ns);
    }

    /// Intent to subscribe (autoSubscribe: the remote publish; manual: the call; tracks already
    /// in the room at join: at connect): opens `lk.subscribe`.
    pub fn subscribe_started(&self, track: SpanTrack) {
        self.0.subscribe_started(track);
    }

    /// The server confirmed the subscription. Without an earlier `subscribe_started` this is the
    /// intent (a fallback, measured from here): `lk.subscribe` opens and `stats_poll_interval_ms`
    /// turns fast at once.
    pub fn subscribed(&self, track: SpanTrack) {
        self.0.subscribed(track);
    }

    /// A track left the room (unpublished, unsubscribed, publisher gone): a pending subscribe
    /// ends, the track's last RTC window ships, its state is forgotten.
    pub fn track_ended(&self, sid: String) {
        self.0.track_ended(&sid);
    }

    /// The subscription failed; `error_type` is the platform's error name.
    pub fn subscribe_failed(&self, sid: String, error_type: String) {
        self.0.subscribe_failed(&sid, &error_type);
    }
}

/// The protocol's `DisconnectReason` number as the shared enum.
#[uniffi::export]
pub fn telemetry_disconnect_reason(proto: i32) -> DisconnectReason {
    DisconnectReason::from_proto(proto)
}

/// The protocol's `ReconnectReason` number (`RR_*`) as the shared enum.
#[uniffi::export]
pub fn telemetry_reconnect_reason(proto: i32) -> ReconnectReason {
    ReconnectReason::from_proto(proto)
}

/// One export the host has to perform on behalf of a pulled pipeline.
#[derive(uniffi::Record)]
pub struct PendingExport {
    pub id: u64,
    pub request: ExportRequest,
}

struct Pending {
    export: PendingExport,
    done: oneshot::Sender<Result<ExportResponse, ExportError>>,
}

/// Pull-side transport: Rust never calls into the host. The exporter queues each request; the
/// host awaits [`next`](Self::next) (a Rust future — those cross every binding) or polls
/// [`try_next`](Self::try_next), performs the HTTP call on its own thread, and reports the
/// outcome with [`complete`](Self::complete),
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
    /// A checkpoint, stamped now.
    pub fn step(&self, step: SpanStep) {
        self.0.step(step);
    }

    /// The open bag; replaces an existing key.
    pub fn set_attribute(&self, key: String, value: AttributeValue) {
        self.0.set_attribute(key, value);
    }

    /// The track the operation is about.
    pub fn set_track(&self, track: SpanTrack) {
        self.0.set_track(track);
    }

    /// End once; `error` becomes `error.type` and the status message.
    pub fn end(&self, outcome: SpanOutcome, error: Option<String>) {
        self.0.end(outcome, error);
    }

    /// End with an error; `error` becomes `error.type`.
    pub fn fail(&self, error: String) {
        self.0.fail(error);
    }

    /// End as cancelled.
    pub fn cancel(&self) {
        self.0.cancel();
    }

    /// Whether the span has ended.
    pub fn is_ended(&self) -> bool {
        self.0.is_ended()
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

/// The pull queue's serving side (see its type docs above [`TelemetrySpan`]).
#[derive(uniffi::Object)]
pub struct TelemetryExportQueue {
    /// Bounded to one request: the exporter has one request out at a time, so nothing piles up
    /// behind a host that stopped serving. `None` once finished.
    tx: Mutex<Option<mpsc::Sender<Pending>>>,
    rx: tokio::sync::Mutex<mpsc::Receiver<Pending>>,
    inflight: Mutex<HashMap<u64, oneshot::Sender<Result<ExportResponse, ExportError>>>>,
    seq: AtomicU64,
    finished: AtomicBool,
    /// Serves the process pipeline (`telemetry_configure_pulled`): nothing is handed out once
    /// the app opted out, even before the purge has cancelled the request.
    process: AtomicBool,
    /// Requests handed to the queue so far (tests only).
    #[cfg(test)]
    queued: AtomicU64,
}

#[uniffi::export(async_runtime = "tokio")]
impl TelemetryExportQueue {
    /// An empty queue; `telemetry_configure_pulled` makes the one it serves.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        let (tx, rx) = mpsc::channel(1);
        Arc::new(Self {
            tx: Mutex::new(Some(tx)),
            rx: tokio::sync::Mutex::new(rx),
            inflight: Mutex::new(HashMap::new()),
            seq: AtomicU64::new(0),
            finished: AtomicBool::new(false),
            process: AtomicBool::new(false),
            #[cfg(test)]
            queued: AtomicU64::new(0),
        })
    }

    /// End the serving loop: `next` resolves `None` from now on, and whatever is still queued is
    /// discarded, never served. Call after `telemetry_shutdown` or `telemetry_disable`, or when a
    /// later `telemetry_configure_pulled` replaced this queue.
    /// (Not `close`: UniFFI's Kotlin objects already have `AutoCloseable.close`.)
    pub fn finish(&self) {
        self.finished.store(true, Ordering::SeqCst);
        self.tx.lock().unwrap_or_else(|e| e.into_inner()).take();
        self.inflight.lock().unwrap_or_else(|e| e.into_inner()).clear();
        // Dropping what is queued tells its exporter the host dropped it.
        if let Ok(mut rx) = self.rx.try_lock() {
            while rx.try_recv().is_ok() {}
        }
    }

    /// The next request to perform; `None` once finished. A request the exporter already gave up
    /// on (timed out, cancelled by shutdown or the opt-out) is never handed out.
    pub async fn next(&self) -> Option<PendingExport> {
        loop {
            if self.finished.load(Ordering::SeqCst) {
                return None;
            }
            let pending = self.rx.lock().await.recv().await?;
            if let Some(export) = self.serve(pending) {
                return Some(export);
            }
            if self.finished.load(Ordering::SeqCst) {
                return None;
            }
        }
    }

    /// The request waiting right now, if any — without waiting: for hosts that must never leave
    /// a pending Rust future holding one of their continuations (Dart: poll from a `Timer`). A
    /// request nobody polls for still times out and is withdrawn exactly as with
    /// [`next`](Self::next); `None` also once finished, or while a `next` is waiting.
    pub fn try_next(&self) -> Option<PendingExport> {
        let mut rx = self.rx.try_lock().ok()?;
        loop {
            if self.finished.load(Ordering::SeqCst) {
                return None;
            }
            if let Some(export) = self.serve(rx.try_recv().ok()?) {
                return Some(export);
            }
        }
    }

    /// The collector's answer to the request with `id`, whatever its status; the core classifies it.
    pub fn complete(&self, id: u64, response: ExportResponse) {
        self.settle(id, Ok(response));
    }

    /// The request with `id` got no answer (network error, timeout, invalid URL).
    pub fn fail(&self, id: u64, error: ExportError) {
        self.settle(id, Err(error));
    }
}

impl TelemetryExportQueue {
    /// Hand `pending` out, unless the queue is finished or its exporter gave up on it.
    fn serve(&self, pending: Pending) -> Option<PendingExport> {
        let opted_out = self.process.load(Ordering::SeqCst) && global::is_disabled();
        if opted_out || self.finished.load(Ordering::SeqCst) || pending.done.is_closed() {
            return None;
        }
        let mut inflight = self.inflight.lock().unwrap_or_else(|e| e.into_inner());
        inflight.retain(|_, done| !done.is_closed());
        inflight.insert(pending.export.id, pending.done);
        Some(pending.export)
    }

    fn settle(&self, id: u64, outcome: Result<ExportResponse, ExportError>) {
        let done = self.inflight.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        if let Some(done) = done {
            let _ = done.send(outcome);
        }
    }

    /// Drop queued requests nobody waits for any more (their exporter timed out or stopped):
    /// with one request out at a time, anything still queued when a new one comes is stale.
    fn discard_stale(&self) {
        let Ok(mut rx) = self.rx.try_lock() else { return };
        let mut live = Vec::new();
        while let Ok(pending) = rx.try_recv() {
            if !pending.done.is_closed() {
                live.push(pending);
            }
        }
        drop(rx);
        let tx = self.tx.lock().unwrap_or_else(|e| e.into_inner()).clone();
        for pending in live {
            if let Some(tx) = &tx {
                let _ = tx.try_send(pending);
            }
        }
    }
}

/// Forgets an in-flight entry when the exporter stops waiting for its answer (timeout,
/// shutdown, opt-out), so nothing is kept for an answer nobody will read.
struct Forget<'a> {
    queue: &'a TelemetryExportQueue,
    id: u64,
}

impl Drop for Forget<'_> {
    fn drop(&mut self) {
        self.queue.inflight.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.id);
    }
}

#[async_trait::async_trait]
impl TelemetryTransport for TelemetryExportQueue {
    async fn send(&self, request: ExportRequest) -> Result<ExportResponse, ExportError> {
        let closed = || ExportError::Retryable {
            reason: "export queue closed".into(),
            retry_after_ms: None,
        };
        self.discard_stale();
        let id = self.seq.fetch_add(1, Ordering::Relaxed);
        let (done, wait) = oneshot::channel();
        let pending = Pending { export: PendingExport { id, request }, done };
        let _forget = Forget { queue: self, id };
        let Some(tx) = self.tx.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
            return Err(closed());
        };
        // Waits for room while the host is busy; cancelled with the exporter's timeout, and then
        // the request was never queued. The sender is not kept while the answer is awaited.
        tx.send(pending).await.map_err(|_| closed())?;
        drop(tx);
        #[cfg(test)]
        self.queued.fetch_add(1, Ordering::SeqCst);
        wait.await.unwrap_or(Err(ExportError::Retryable {
            reason: "host dropped the export".into(),
            retry_after_ms: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn request() -> ExportRequest {
        ExportRequest {
            url: "https://p.livekit.cloud/observability/client/logs/otlp/v0".into(),
            headers: HashMap::from([("Authorization".into(), "Bearer secret".into())]),
            body: vec![1, 2, 3],
        }
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().enable_time().build().expect("runtime")
    }

    /// Finding r1-4: a host that stopped serving never gets a pile of stale requests — and
    /// their tokens — to send later: requests the exporter gave up on are discarded, not served.
    #[test]
    fn a_suspended_host_is_never_served_stale_requests() {
        runtime().block_on(async {
            let queue = TelemetryExportQueue::new();
            for _ in 0..5 {
                let sent = tokio::time::timeout(Duration::from_millis(10), queue.send(request()));
                assert!(sent.await.is_err(), "the exporter's timeout gives up");
            }
            assert!(queue.inflight.lock().expect("lock").is_empty(), "nothing kept for answers");
            let served = tokio::time::timeout(Duration::from_millis(50), queue.next()).await;
            assert!(served.is_err(), "no stale request is handed out");
        });
    }

    /// Finding r1-4: a request cancelled before the host asked for it (shutdown, opt-out) is
    /// never served; a live one still is.
    #[test]
    fn a_cancelled_request_is_never_served() {
        runtime().block_on(async {
            let queue = TelemetryExportQueue::new();
            let cancelled = {
                let queue = queue.clone();
                tokio::spawn(async move { queue.send(request()).await })
            };
            tokio::task::yield_now().await;
            cancelled.abort();
            let _ = cancelled.await;
            let live = {
                let queue = queue.clone();
                tokio::spawn(async move { queue.send(request()).await })
            };
            tokio::task::yield_now().await;
            let served = queue.next().await.expect("the live request");
            queue.complete(served.id, ExportResponse::accepted());
            assert_eq!(live.await.expect("task"), Ok(ExportResponse::accepted()));
        });
    }

    /// Finding r1-4: finishing the queue discards what is queued instead of serving it.
    #[test]
    fn finishing_discards_what_is_queued() {
        runtime().block_on(async {
            let queue = TelemetryExportQueue::new();
            let waiting = {
                let queue = queue.clone();
                tokio::spawn(async move { queue.send(request()).await })
            };
            tokio::task::yield_now().await;
            queue.finish();
            assert!(queue.next().await.is_none(), "not served after finish");
            assert!(waiting.await.expect("task").is_err(), "the exporter hears it was dropped");
        });
    }

    /// Flutter review: `try_next` never waits. It serves a live request, never one its exporter
    /// gave up on while nobody polled, and nothing once finished.
    #[test]
    fn try_next_serves_without_waiting_and_never_a_stale_request() {
        runtime().block_on(async {
            let queue = TelemetryExportQueue::new();
            assert!(queue.try_next().is_none(), "empty: returns at once");
            let sent = tokio::time::timeout(Duration::from_millis(10), queue.send(request()));
            assert!(sent.await.is_err(), "nobody polled: the exporter's timeout gives up");
            assert!(queue.try_next().is_none(), "the withdrawn request is never served");
            let live = {
                let queue = queue.clone();
                tokio::spawn(async move { queue.send(request()).await })
            };
            tokio::task::yield_now().await;
            let served = queue.try_next().expect("the live request");
            queue.complete(served.id, ExportResponse::accepted());
            assert_eq!(live.await.expect("task"), Ok(ExportResponse::accepted()));
            queue.complete(served.id, ExportResponse::accepted()); // stale id: ignored
            queue.fail(
                u64::MAX,
                ExportError::Retryable { reason: "x".into(), retry_after_ms: None },
            );
            queue.finish();
            assert!(queue.try_next().is_none());
        });
    }

    /// An unsigned participant token with the observability grant (the core reads claims, never
    /// verifies them).
    fn granted_token() -> String {
        fn b64url(bytes: &[u8]) -> String {
            const ABC: &[u8; 64] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut out = String::new();
            for chunk in bytes.chunks(3) {
                let n = chunk
                    .iter()
                    .enumerate()
                    .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
                for i in 0..=chunk.len() {
                    out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
                }
            }
            out
        }
        let exp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs()
            + 3600;
        let claims = format!(r#"{{"exp":{exp},"observability":{{"write":true}}}}"#);
        format!("{}.{}.sig", b64url(br#"{"alg":"HS256"}"#), b64url(claims.as_bytes()))
    }

    /// Wait (bounded) until `condition` holds.
    fn eventually(condition: impl Fn() -> bool) -> bool {
        (0..500).any(|_| {
            condition() || {
                std::thread::sleep(Duration::from_millis(10));
                false
            }
        })
    }

    /// Final review: a second opt-out while the first one's purge is blocked does not let a
    /// waiter (`telemetry_flush`/`telemetry_shutdown`) return before that first purge finishes.
    #[test]
    fn a_repeated_opt_out_waits_for_the_first_purge() {
        static SLOT: PurgeSlot = Mutex::new(None);
        let (release, blocked) = oneshot::channel::<()>();
        start_purge(&SLOT, blocked);
        start_purge(&SLOT, async {});
        runtime().block_on(async {
            let early = tokio::time::timeout(Duration::from_millis(200), wait_purge(&SLOT)).await;
            assert!(early.is_err(), "the first purge is still running");
            release.send(()).expect("first purge waiting");
            tokio::time::timeout(Duration::from_secs(5), wait_purge(&SLOT))
                .await
                .expect("done once the first purge is");
        });
    }

    /// Finding r2-2 + Swift review: the opt-out is in effect when `telemetry_disable` returns —
    /// no scope, nothing captured — and once its purge is awaited (`telemetry_shutdown`) the
    /// pulled request of a pipeline replaced while its upload was pending is never handed to the
    /// host; `telemetry_is_disabled` turns `true` for every thread. (The only test here that
    /// touches the process-wide pipeline: the opt-out is one-way.)
    #[test]
    fn the_opt_out_withdraws_a_replaced_generations_pulled_request() {
        let config = || TelemetryConfig { resource: Vec::new(), ..Default::default() };
        let first = telemetry_configure_pulled(config(), Vec::new());
        let room = telemetry_scope().expect("scope");
        room.set_server("wss://p.livekit.cloud".into(), granted_token());
        room.emit_custom("ping".into(), HashMap::new());
        crate::runtime::runtime().spawn(telemetry_flush());
        assert!(
            eventually(|| first.queued.load(Ordering::SeqCst) == 1),
            "precondition: a pulled request is pending in the first queue"
        );
        let _second = telemetry_configure_pulled(config(), Vec::new()); // `first` now drains
        assert!(!telemetry_is_disabled());
        telemetry_disable();
        assert!(telemetry_is_disabled(), "visible process-wide once the call returns");
        assert!(std::thread::spawn(telemetry_is_disabled).join().expect("thread"));
        assert!(first.try_next().is_none(), "nothing is served once the call returns");
        assert!(telemetry_scope().is_none(), "no scope once the call returns");
        assert!(telemetry_stats().is_none());
        room.emit_custom("after".into(), HashMap::new()); // refused: its generation is revoked
                                                          // A configure after the opt-out deletes what its storage dir still holds and starts no
                                                          // exporter: the queue it hands back is held by nothing else.
        let dir = std::env::temp_dir().join(format!("lk-refused-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).expect("dir");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::fs::write(dir.join(format!("{stamp:020}-000001-1-l.otlp")), b"cached").expect("batch");
        let refused = telemetry_configure_pulled(
            TelemetryConfig { storage_dir: Some(dir.to_string_lossy().into_owned()), ..config() },
            Vec::new(),
        );
        assert_eq!(Arc::strong_count(&refused), 1, "no exporter holds its transport");
        assert_eq!(std::fs::read_dir(&dir).expect("dir").count(), 0, "the cache is purged");
        let _ = std::fs::remove_dir_all(&dir);
        runtime().block_on(async {
            telemetry_shutdown().await; // awaits the purge
            let served = tokio::time::timeout(Duration::from_millis(300), first.next()).await;
            assert!(!matches!(served, Ok(Some(_))), "withdrawn, never served");
        });
    }
}
