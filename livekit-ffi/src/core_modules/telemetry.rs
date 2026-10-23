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
use tokio::sync::{mpsc, oneshot};

/// Why a [`livekit_telemetry::Telemetry`] pipeline could not be created.
#[derive(uniffi::Error, thiserror::Error, Debug)]
#[uniffi(flat_error)]
pub enum TelemetryError {
    /// No transport was passed and no HTTP client is registered with `livekit-net`.
    #[error("no telemetry transport: pass one, or register an HTTP client with livekit-net first")]
    NoTransport,
}

/// Start the process pipeline; a previous one is drained and replaced. `transport = None` uses
/// the HTTP client registered with `livekit-net`, if any. After [`telemetry_disable`] nothing
/// starts, and a configure that gets as far as installing deletes whatever `config.storage_dir`
/// still holds.
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
    install(config, transport, instruments);
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
    if !install(config, queue.clone(), instruments) {
        // No exporter will ever queue a request: `next` must not wait for one.
        queue.finish();
    }
    queue
}

/// Install the pipeline through the process lifecycle; `false` when the install was refused
/// (after the opt-out) and no exporter runs.
fn install(
    config: TelemetryConfig,
    transport: Arc<dyn TelemetryTransport>,
    instruments: Vec<Arc<dyn TelemetryInstrument>>,
) -> bool {
    let (telemetry, exporter) = livekit_telemetry::Telemetry::new(config, transport);
    configure_in(&LIFECYCLE, telemetry, exporter, move |telemetry| {
        // The pipeline it replaces is the one the lifecycle runs, and is stopped there in turn.
        global::install(telemetry, instruments);
        // Refused after the opt-out (the new pipeline was purged instead, its `storage_dir` with
        // it), or opted out meanwhile (every generation revoked): nothing to run.
        !global::is_disabled()
    })
}

/// Have the lifecycle in `slot` install `telemetry` with `install` and run its exporter in turn,
/// and wait until it installed it: `false`, and no exporter, when `install` refused it. The
/// lifecycle installs on its own thread, so that is where the instruments start; it answers
/// before any drain (its exporter runner drains), so this waits only for the commands before it.
fn configure_in(
    slot: &LifecycleSlot,
    telemetry: livekit_telemetry::Telemetry,
    exporter: livekit_telemetry::Exporter,
    install: impl FnOnce(livekit_telemetry::Telemetry) -> bool + Send + 'static,
) -> bool {
    // A blocking wait: `telemetry_configure` is synchronous.
    let (done, installed) = std::sync::mpsc::sync_channel(1);
    let exporter = Box::new(exporter);
    slot.send(Command::Configure { install: Box::new(install), telemetry, exporter, done });
    installed.recv().unwrap_or(false)
}

/// One lifecycle operation. A lifecycle carries them out one at a time, in the order sent.
enum Command {
    /// Install a pipeline with `install` and run its exporter once the one before it stopped;
    /// answers whether it was installed.
    Configure {
        install: Box<dyn FnOnce(livekit_telemetry::Telemetry) -> bool + Send>,
        telemetry: livekit_telemetry::Telemetry,
        /// Boxed: an exporter is large.
        exporter: Box<livekit_telemetry::Exporter>,
        done: std::sync::mpsc::SyncSender<bool>,
    },
    /// Uninstall with `uninstall`, then answer once every exporter queued before has stopped.
    Shutdown { uninstall: Box<dyn FnOnce() + Send>, done: oneshot::Sender<()> },
    /// The opt-out's purge, revoked already: the commands after it wait for it.
    Purge(std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>),
    /// Answers once every command before it is done.
    Barrier(oneshot::Sender<()>),
}

/// Where a lifecycle lives: the running one, held by its owner, if any.
struct LifecycleSlot {
    lifecycle: Mutex<Option<LifecycleHandle>>,
    /// Lifecycles started in this slot so far: each new one's generation is the next number.
    generations: std::sync::atomic::AtomicU64,
    /// The runtime it runs on instead of the bridge's (tests only: so they can count its tasks).
    #[cfg(test)]
    runtime: Option<tokio::runtime::Handle>,
}

/// The process pipeline's lifecycle.
static LIFECYCLE: LifecycleSlot = LifecycleSlot::new();

impl LifecycleSlot {
    const fn new() -> Self {
        Self {
            lifecycle: Mutex::new(None),
            generations: std::sync::atomic::AtomicU64::new(0),
            #[cfg(test)]
            runtime: None,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<LifecycleHandle>> {
        self.lifecycle.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Send `command`, starting a lifecycle when none runs; returns its ticket.
    fn send(&self, command: Command) -> Ticket {
        let mut lifecycle = self.lock();
        lifecycle.get_or_insert_with(|| self.spawn()).send(command)
    }

    /// Send `command` to the running lifecycle; `None`, and nothing sent, when none runs.
    fn send_if_running(&self, command: Command) -> Option<Ticket> {
        self.lock().as_mut().map(|lifecycle| lifecycle.send(command))
    }

    /// A new lifecycle, of the next generation, on the bridge runtime.
    fn spawn(&self) -> LifecycleHandle {
        let generation = self.generations.fetch_add(1, Ordering::SeqCst) + 1;
        #[cfg(test)]
        if let Some(runtime) = &self.runtime {
            return Lifecycle::spawn(runtime, generation);
        }
        Lifecycle::spawn(crate::runtime::runtime().handle(), generation)
    }

    /// Take the lifecycle out for its owner to end, only when the slot still holds the lifecycle
    /// `ticket` went to and that command is still its last one. A ticket of an earlier lifecycle
    /// never matches a later one, whatever its command number.
    fn take_after(&self, ticket: Ticket) -> Option<LifecycleHandle> {
        self.lock().take_if(|lifecycle| lifecycle.ticket() == ticket)
    }
}

/// A running lifecycle, as its owner holds it.
struct LifecycleHandle {
    commands: mpsc::UnboundedSender<Command>,
    task: Owned,
    /// Which lifecycle of its slot this is: no other lifecycle of the slot has the same one.
    generation: u64,
    /// Commands sent so far. A shutdown ends this lifecycle only when the slot still holds it
    /// and the shutdown's command is still the last one sent (see [`LifecycleSlot::take_after`]).
    sent: u64,
}

/// A sent command: the generation of the lifecycle it went to, and its number there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ticket {
    generation: u64,
    command: u64,
}

/// How long a lifecycle may take to end once its commands are gone: it has stopped every exporter
/// by then, so it ends at once, and this bound only guards against a stuck one.
const END_BOUND: std::time::Duration = std::time::Duration::from_secs(1);

impl LifecycleHandle {
    /// Send `command`; returns its ticket.
    fn send(&mut self, command: Command) -> Ticket {
        self.sent += 1;
        // A lifecycle ends only once its owner dropped this sender: the command arrives.
        let _ = self.commands.send(command);
        self.ticket()
    }

    /// The ticket of the last command sent.
    fn ticket(&self) -> Ticket {
        Ticket { generation: self.generation, command: self.sent }
    }

    /// End the lifecycle and join it: without commands it ends its exporter runner and returns.
    /// Past [`END_BOUND`] it is cancelled and joined all the same.
    async fn end(self) {
        drop(self.commands);
        self.task.join(END_BOUND, async {}).await;
    }
}

/// A task its owner joins. Dropped unjoined (its owner was cancelled), it is cancelled too, never
/// left running on its own.
struct Owned(tokio::task::JoinHandle<()>);

impl Owned {
    /// Run `first`, then wait for the task, both within `bound`; past it, cancel the task and
    /// wait until it is gone.
    async fn join(mut self, bound: std::time::Duration, first: impl std::future::Future) {
        let joined = tokio::time::timeout(bound, async {
            first.await;
            let _ = (&mut self.0).await;
        })
        .await;
        if joined.is_err() {
            self.0.abort();
            let _ = (&mut self.0).await;
        }
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The bridge's lifecycle owner: carries out every lifecycle operation (install, uninstall, the
/// opt-out's purge) in the order its commands were sent, so a configure and a shutdown never
/// interleave. Each installed pipeline's exporter goes to its exporter runner, which starts one
/// only once the one before it stopped; the lifecycle never waits for a drain itself, so a
/// configure waits only for the commands before it.
struct Lifecycle {
    commands: mpsc::UnboundedReceiver<Command>,
    exporters: mpsc::UnboundedSender<Step>,
}

impl Lifecycle {
    /// Start a lifecycle and its exporter runner on `runtime`. The channels are unbounded:
    /// `telemetry_configure` and `telemetry_disable` are synchronous and must neither block nor
    /// fail to queue, and a command stands for one such call.
    fn spawn(runtime: &tokio::runtime::Handle, generation: u64) -> LifecycleHandle {
        let (sender, commands) = mpsc::unbounded_channel();
        let task = runtime.spawn(async move {
            let (exporters, steps) = mpsc::unbounded_channel();
            let runner = Owned(tokio::spawn(Exporters { steps, running: None }.run()));
            Self { commands, exporters }.run(runner).await;
        });
        LifecycleHandle { commands: sender, task: Owned(task), generation, sent: 0 }
    }

    async fn run(mut self, runner: Owned) {
        while let Some(command) = self.commands.recv().await {
            match command {
                Command::Configure { install, telemetry, exporter, done } => {
                    let installed = install(telemetry.clone());
                    if installed {
                        let _ = self.exporters.send(Step::Run(telemetry, exporter));
                    } else {
                        // Gone before the answer: a refused pulled queue is held by its caller only.
                        drop((telemetry, exporter));
                    }
                    let _ = done.send(installed);
                }
                Command::Shutdown { uninstall, done } => {
                    uninstall();
                    let _ = self.exporters.send(Step::Stop(done));
                }
                Command::Purge(purge) => purge.await,
                Command::Barrier(done) => {
                    let _ = done.send(());
                }
            }
        }
        // No command can come any more: the runner stops what it runs and ends.
        drop(self.exporters);
        runner.join(END_BOUND, async {}).await;
    }
}

/// One request to the [`Exporters`] runner.
enum Step {
    /// Stop the running exporter, then run this pipeline's.
    Run(livekit_telemetry::Telemetry, Box<livekit_telemetry::Exporter>),
    /// Stop the running exporter, then answer.
    Stop(oneshot::Sender<()>),
}

/// A lifecycle's exporter runner: one exporter at a time, in the order the steps come. A
/// pipeline's exporter starts only once the one before it is gone, drained or else cancelled and
/// joined at its bound, so two exporters never replay a shared cache at the same time and none
/// outlives its stop. New captures wait in the new pipeline's queue meanwhile.
struct Exporters {
    steps: mpsc::UnboundedReceiver<Step>,
    /// The running exporter's pipeline and task.
    running: Option<(livekit_telemetry::Telemetry, Owned)>,
}

impl Exporters {
    async fn run(mut self) {
        while let Some(step) = self.steps.recv().await {
            self.stop().await;
            match step {
                Step::Run(telemetry, exporter) => {
                    self.running = Some((telemetry, Owned(tokio::spawn(exporter.run()))));
                }
                Step::Stop(done) => {
                    let _ = done.send(());
                }
            }
        }
        self.stop().await;
    }

    /// Drain the running pipeline and join its exporter, within the pipeline's shutdown bound.
    /// Past it the exporter is cancelled and joined all the same: dropping its handle would
    /// leave it running.
    async fn stop(&mut self) {
        let Some((telemetry, task)) = self.running.take() else { return };
        task.join(telemetry.shutdown_bound(), telemetry.shutdown()).await;
    }
}

/// Drain and stop the process pipeline; every call is a no-op afterwards. After
/// [`telemetry_disable`], returns once its purge is complete. Returns once every exporter
/// started before the call has stopped as well: each drains within its pipeline's shutdown bound
/// (its export timeout plus 1 s) or is cancelled there, so right after a replace this waits one
/// bound for each pipeline still in line. A shutdown that is the last call ends the lifecycle
/// and joins it; the next configure starts a fresh one.
#[uniffi::export(async_runtime = "tokio")]
pub async fn telemetry_shutdown() {
    shutdown_in(&LIFECYCLE, || {
        // Its exporter is stopped by the lifecycle, in turn.
        global::uninstall();
    })
    .await;
}

/// Have the lifecycle in `slot` uninstall with `uninstall` and stop every exporter queued before,
/// and wait for that. Then, when the slot still holds that same lifecycle and this was its last
/// command, end the lifecycle and join it.
async fn shutdown_in(slot: &LifecycleSlot, uninstall: impl FnOnce() + Send + 'static) {
    let (done, stopped) = oneshot::channel();
    let command = Command::Shutdown { uninstall: Box::new(uninstall), done };
    // No lifecycle: nothing was installed through one, nothing runs.
    let Some(ticket) = slot.send_if_running(command) else { return };
    let _ = stopped.await;
    if let Some(lifecycle) = slot.take_after(ticket) {
        lifecycle.end().await;
    }
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
    // The revocation holds on return; the purge then runs on the lifecycle, in turn.
    let purge = global::disable();
    LIFECYCLE.send(Command::Purge(Box::pin(async move {
        purge.await;
    })));
}

/// Whether the process opted out with [`telemetry_disable`]: `true` for every thread and isolate
/// from the moment that call revokes collection, before it returns. A cheap read that never
/// panics; platforms with per-isolate state may check it right before each `getStats` or submit.
#[uniffi::export]
pub fn telemetry_is_disabled() -> bool {
    global::is_disabled()
}

/// Wait until every command sent to the lifecycle in `slot` so far is done; at once when none
/// runs.
async fn barrier(slot: &LifecycleSlot) {
    let (done, passed) = oneshot::channel();
    if slot.send_if_running(Command::Barrier(done)).is_some() {
        let _ = passed.await;
    }
}

/// Cache everything queued and upload what the network allows. After [`telemetry_disable`],
/// returns once its purge is complete.
#[uniffi::export(async_runtime = "tokio")]
pub async fn telemetry_flush() {
    barrier(&LIFECYCLE).await;
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
/// forwarder, which the core has already copied (`log_forward_bootstrap`). Credentials in the
/// line (a compact JWT, the value after `Bearer` or a `token` key) are masked, as in the core's
/// own copied lines.
#[uniffi::export]
pub fn telemetry_log(record: LogRecord) {
    global::log(masked(record));
}

/// `record` with its credentials masked: a token quoted in a platform error never leaves the
/// device.
fn masked(mut record: LogRecord) -> LogRecord {
    record.body = crate::log_forward::mask_credentials(&record.body);
    record
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

/// SDK metadata added at export to every record of every scope (not an app API: app
/// attributes are room-scoped, [`TelemetryScope::set_attribute`]); `None` removes it. Keys up to
/// 128 bytes, values up to 1024, at most 64 keys, never `session.id`, `error.type` or
/// `lk.outcome`; anything else is rejected and counted.
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
    /// and keys up to 128 bytes, values up to 1024, at most 64 attributes, none of the SDK's
    /// keys (`lk.*`, `otel.*`, `code.*`, `session.id`, `error.type`, `network.connection.type`);
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

    /// The room and local participant, on every record this session captures from now on.
    pub fn set_room(&self, room: RoomIdentity) {
        self.0.set_room(room);
    }

    /// The session ended for good (not a reconnect): `lk.room.disconnected`, once; a repeated call
    /// is a no-op until the session starts a new `lk.connect`.
    pub fn disconnected(&self, reason: DisconnectReason) {
        self.0.disconnected(reason);
    }

    /// A log record filed under this session even without an ambient span. Credentials in it are
    /// masked, as with [`telemetry_log`].
    pub fn log(&self, record: LogRecord) {
        self.0.log(masked(record));
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
    DisconnectReason::from(proto)
}

/// The protocol's `ReconnectReason` number (`RR_*`) as the shared enum.
#[uniffi::export]
pub fn telemetry_reconnect_reason(proto: i32) -> ReconnectReason {
    ReconnectReason::from(proto)
}

/// One export the host has to perform on behalf of a pulled pipeline.
#[derive(uniffi::Record)]
pub struct PendingExport {
    /// What [`TelemetryExportQueue::complete`] and [`TelemetryExportQueue::fail`] take to answer
    /// this request.
    pub id: u64,
    /// The request to POST, exactly as given.
    pub request: ExportRequest,
}

/// A queued export and the channel its exporter awaits the answer on.
struct QueuedExport {
    /// What the host is handed.
    export: PendingExport,
    /// The exporter's end: closed once it gave up on the request.
    done: oneshot::Sender<Result<ExportResponse, ExportError>>,
}

/// One attempt at an SDK operation, owned by the core. Every call is synchronous and stamps the
/// clock inside, so the only skew is the FFI call; `describe()` is the console line on every
/// platform. Detached (no session) it still times and describes itself.
#[derive(uniffi::Object)]
pub struct TelemetrySpan(Arc<livekit_telemetry::Span>);

#[uniffi::export]
impl TelemetrySpan {
    /// A checkpoint, stamped now. Ignored once the span has ended.
    pub fn step(&self, step: SpanStep) {
        self.0.step(step);
    }

    /// The open bag; replaces an existing key. An app-defined span refuses the SDK's keys.
    pub fn set_attribute(&self, key: String, value: AttributeValue) {
        self.0.set_attribute(key, value);
    }

    /// The track the operation is about.
    pub fn set_track(&self, track: SpanTrack) {
        self.0.set_track(track);
    }

    /// End once; every later call is ignored. `error` becomes `error.type` when the outcome is
    /// `Error`.
    pub fn end(&self, outcome: SpanOutcome, error: Option<String>) {
        self.0.end(outcome, error);
    }

    /// End with an error; `error` becomes `error.type`.
    pub fn fail(&self, error: String) {
        self.0.fail(error);
    }

    /// End as cancelled: the status stays unset and `lk.outcome` is `cancelled`.
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

/// Pull-side transport: Rust never calls into the host. The exporter queues each request; the
/// host awaits [`next`](Self::next) (a Rust future — those cross every binding) or polls
/// [`try_next`](Self::try_next), performs the HTTP call on its own thread, and reports the
/// outcome with [`complete`](Self::complete), which unblocks the exporter's retry/drop/go-silent
/// logic exactly as a direct transport would.
///
/// Exists because uniffi-dart's foreign-trait callbacks are isolate-bound (`Pointer.fromFunction`)
/// and abort the VM when invoked from a tokio thread; Swift and Kotlin callbacks are thread-agnostic
/// and use [`TelemetryTransport`] directly. Only [`telemetry_configure_pulled`] makes one.
#[derive(uniffi::Object)]
pub struct TelemetryExportQueue {
    /// Bounded to one request: the exporter has one request out at a time, so nothing piles up
    /// behind a host that stopped serving. `None` once finished.
    tx: Mutex<Option<mpsc::Sender<QueuedExport>>>,
    /// The requests waiting for the host.
    rx: tokio::sync::Mutex<mpsc::Receiver<QueuedExport>>,
    /// The requests handed out and not answered yet, by id. `serve` and `finish` read `finished`
    /// under this lock, so nothing is handed out once `finish` returned.
    inflight: Mutex<HashMap<u64, oneshot::Sender<Result<ExportResponse, ExportError>>>>,
    /// The next request id.
    seq: AtomicU64,
    /// `finish` was called: nothing is served from now on.
    finished: AtomicBool,
    /// Serves the process pipeline (`telemetry_configure_pulled`): nothing is handed out once
    /// the app opted out, even before the purge has cancelled the request. `false` only for the
    /// unit tests' own queues.
    process: AtomicBool,
    /// Requests handed to the queue so far (tests only).
    #[cfg(test)]
    queued: AtomicU64,
    /// Runs inside `serve` between its checks and the hand-out (tests only: holds a serve there
    /// while `finish` races it).
    #[cfg(test)]
    before_serve: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

#[uniffi::export(async_runtime = "tokio")]
impl TelemetryExportQueue {
    /// End the serving loop: `next` resolves `None` from now on, and whatever is still queued is
    /// discarded, never served. Call after `telemetry_shutdown` or `telemetry_disable`, or when a
    /// later `telemetry_configure_pulled` replaced this queue.
    /// (Not `close`: UniFFI's Kotlin objects already have `AutoCloseable.close`.)
    pub fn finish(&self) {
        {
            let mut inflight = self.inflight.lock().unwrap_or_else(|e| e.into_inner());
            self.finished.store(true, Ordering::SeqCst);
            inflight.clear();
        }
        self.tx.lock().unwrap_or_else(|e| e.into_inner()).take();
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
            let queued = self.rx.lock().await.recv().await?;
            if let Some(export) = self.serve(queued) {
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

    /// The collector's answer to the request with `id`, whatever its status; the core classifies
    /// it. An unknown or stale `id` (already answered, or given up on) is ignored.
    pub fn complete(&self, id: u64, response: ExportResponse) {
        self.settle(id, Ok(response));
    }

    /// The request with `id` got no answer (network error, timeout, invalid URL). An unknown or
    /// stale `id` is ignored.
    pub fn fail(&self, id: u64, error: ExportError) {
        self.settle(id, Err(error));
    }
}

impl TelemetryExportQueue {
    /// An empty queue.
    fn new() -> Arc<Self> {
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
            #[cfg(test)]
            before_serve: Mutex::new(None),
        })
    }

    /// Hand `queued` out, unless the queue is finished or its exporter gave up on it.
    fn serve(&self, queued: QueuedExport) -> Option<PendingExport> {
        let mut inflight = self.inflight.lock().unwrap_or_else(|e| e.into_inner());
        let opted_out = self.process.load(Ordering::SeqCst) && global::is_disabled();
        if opted_out || self.finished.load(Ordering::SeqCst) || queued.done.is_closed() {
            return None;
        }
        #[cfg(test)]
        if let Some(hook) = self.before_serve.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            hook();
        }
        inflight.retain(|_, done| !done.is_closed());
        inflight.insert(queued.export.id, queued.done);
        Some(queued.export)
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
        while let Ok(queued) = rx.try_recv() {
            if !queued.done.is_closed() {
                live.push(queued);
            }
        }
        drop(rx);
        let tx = self.tx.lock().unwrap_or_else(|e| e.into_inner()).clone();
        for queued in live {
            if let Some(tx) = &tx {
                let _ = tx.try_send(queued);
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
        let queued = QueuedExport { export: PendingExport { id, request }, done };
        let _forget = Forget { queue: self, id };
        let Some(tx) = self.tx.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
            return Err(closed());
        };
        // Waits for room while the host is busy; cancelled with the exporter's timeout, and then
        // the request was never queued. The sender is not kept while the answer is awaited.
        tx.send(queued).await.map_err(|_| closed())?;
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

    /// A host that stopped serving never gets a pile of stale requests — and their tokens — to send
    /// later: requests the exporter gave up on are discarded, not served.
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

    /// A request cancelled before the host asked for it (shutdown, opt-out) is never served; a live
    /// one still is.
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

    /// Finishing the queue discards what is queued instead of serving it.
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

    /// `try_next` never waits. It serves a live request, never one its exporter gave up on while
    /// nobody polled, and nothing once finished.
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

    /// `finish` and `serve` take the in-flight lock in turn: once `finish` returned, nothing is
    /// handed out or kept, even by a `serve` that passed its checks before.
    #[test]
    fn nothing_is_served_after_finish_returns() {
        let queue = TelemetryExportQueue::new();
        let (entered, inside) = std::sync::mpsc::channel();
        let (resume, resumed) = std::sync::mpsc::channel::<()>();
        let resumed = Mutex::new(resumed);
        *queue.before_serve.lock().expect("lock") = Some(Box::new(move || {
            let _ = entered.send(());
            let _ = resumed.lock().expect("lock").recv_timeout(Duration::from_millis(200));
        }));
        let (done, _answer) = oneshot::channel();
        let queued = QueuedExport { export: PendingExport { id: 7, request: request() }, done };
        let serving = {
            let queue = queue.clone();
            std::thread::spawn(move || queue.serve(queued))
        };
        inside.recv().expect("serve passed its checks");
        queue.finish();
        let _ = resume.send(());
        serving.join().expect("serve");
        assert!(queue.inflight.lock().expect("lock").is_empty(), "nothing kept after finish");
    }

    /// A platform's log line loses its credentials before it is captured: the upload carries the
    /// mask, never the token.
    #[test]
    fn platform_log_lines_are_masked() {
        let token = granted_token();
        let kept = Arc::new(Gated {
            gate: tokio::sync::Semaphore::new(1_000),
            bodies: Mutex::new(Vec::new()),
        });
        runtime().block_on(async {
            let (telemetry, exporter) =
                livekit_telemetry::Telemetry::new(TelemetryConfig::default(), kept.clone());
            tokio::spawn(exporter.run());
            let scope = TelemetryScope(telemetry.begin_scope());
            scope.set_server("wss://p.livekit.cloud".into(), token.clone());
            scope.log(LogRecord {
                severity: livekit_telemetry::Severity::Warn,
                source: livekit_telemetry::LogSource::Sdk,
                body: format!("connect failed: wss://p.livekit.cloud/rtc?access_token={token}"),
                logger: None,
                function: None,
                file: None,
                line: None,
                timestamp_ns: None,
                span_id: None,
            });
            telemetry.flush().await;
        });
        let mut sent = Vec::new();
        for body in kept.bodies.lock().expect("lock").iter() {
            let mut gunzip = flate2::read::GzDecoder::new(body.as_slice());
            std::io::Read::read_to_end(&mut gunzip, &mut sent).expect("gzip");
        }
        let has = |text: &str| sent.windows(text.len()).any(|w| w == text.as_bytes());
        assert!(has("access_token=<redacted>"), "the line went out, masked");
        assert!(!has(&token), "the token never leaves the device");
    }

    /// A transport that holds every request until the test opens its gate, and keeps the bodies.
    struct Gated {
        gate: tokio::sync::Semaphore,
        bodies: Mutex<Vec<Vec<u8>>>,
    }

    #[async_trait::async_trait]
    impl TelemetryTransport for Gated {
        async fn send(&self, request: ExportRequest) -> Result<ExportResponse, ExportError> {
            self.bodies.lock().expect("lock").push(request.body);
            let _open = self.gate.acquire().await;
            Ok(ExportResponse::accepted())
        }
    }

    /// The process pipeline's slot, for `configure` and `shut_down`.
    type Installed = Mutex<Option<livekit_telemetry::Telemetry>>;

    /// What `install` does, with `installed` for the process pipeline: `pause` runs in the
    /// lifecycle right after the install.
    fn configure(
        slot: &LifecycleSlot,
        installed: &Arc<Installed>,
        telemetry: livekit_telemetry::Telemetry,
        exporter: livekit_telemetry::Exporter,
        pause: impl FnOnce() + Send + 'static,
    ) {
        let installed = installed.clone();
        configure_in(slot, telemetry, exporter, move |telemetry| {
            installed.lock().expect("lock").replace(telemetry);
            pause();
            true
        });
    }

    /// What `telemetry_shutdown` does, with `installed` for the process pipeline.
    async fn shut_down(slot: &LifecycleSlot, installed: &Arc<Installed>) {
        let installed = installed.clone();
        shutdown_in(slot, move || {
            installed.lock().expect("lock").take();
        })
        .await;
    }

    /// What the exporter runner does when it stops a running exporter.
    async fn stop_running(
        telemetry: livekit_telemetry::Telemetry,
        task: tokio::task::JoinHandle<()>,
    ) {
        let steps = mpsc::unbounded_channel().1;
        Exporters { steps, running: Some((telemetry, Owned(task))) }.stop().await;
    }

    /// A slot whose lifecycles run on `runtime`, so a test can count their tasks.
    fn slot_on(runtime: &tokio::runtime::Runtime) -> LifecycleSlot {
        LifecycleSlot { runtime: Some(runtime.handle().clone()), ..LifecycleSlot::new() }
    }

    /// The tasks alive on the runtime `slot`'s lifecycles run on.
    fn alive_tasks(slot: &LifecycleSlot) -> usize {
        let bridge = crate::runtime::runtime().handle();
        slot.runtime.as_ref().unwrap_or(bridge).metrics().num_alive_tasks()
    }

    /// Wait until `sent` commands went to the lifecycle in `slot`.
    fn wait_until_sent(slot: &LifecycleSlot, sent: u64) {
        let reached = || slot.lock().as_ref().is_some_and(|lifecycle| lifecycle.sent == sent);
        assert!(eventually(reached), "{sent} commands sent");
    }

    /// A replacing pipeline's exporter starts only once the previous one drained: the two never
    /// replay a shared cache at the same time, so a batch in flight is not sent twice.
    #[test]
    fn a_replacing_exporter_waits_for_the_previous_drain() {
        static SLOT: LifecycleSlot = LifecycleSlot::new();
        let installed = Arc::new(Installed::default());
        let dir = std::env::temp_dir().join(format!("lk-hand-over-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let config = || TelemetryConfig {
            storage_dir: Some(dir.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let transport = Arc::new(Gated {
            gate: tokio::sync::Semaphore::new(0),
            bodies: Mutex::new(Vec::new()),
        });
        let sent = || transport.bodies.lock().expect("lock").clone();
        runtime().block_on(async {
            let (first, exporter) = livekit_telemetry::Telemetry::new(config(), transport.clone());
            configure(&SLOT, &installed, first.clone(), exporter, || ());
            let room = first.begin_scope();
            room.set_server("wss://p.livekit.cloud", &granted_token());
            room.emit_custom("ping", Vec::new());
            let flushing = tokio::spawn(async move { first.flush().await });
            while sent().is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            let in_flight = sent()[0].clone();
            let (second, exporter) = livekit_telemetry::Telemetry::new(config(), transport.clone());
            second.begin_scope().set_server("wss://p.livekit.cloud", &granted_token());
            configure(&SLOT, &installed, second.clone(), exporter, || ());
            // Long enough for a second exporter to replay the cache, were it running.
            tokio::time::sleep(Duration::from_millis(100)).await;
            transport.gate.add_permits(1_000);
            flushing.await.expect("flush");
            second.flush().await;
            shut_down(&SLOT, &installed).await;
            let copies = sent().iter().filter(|body| **body == in_flight).count();
            assert_eq!(copies, 1, "the batch in flight is sent once");
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two replaces while the first pipeline drains an upload held for 4.5 s (its export timeout
    /// is 20 s): each later exporter starts only once the one before it stopped, however short its
    /// own bound, so the batch the first has in flight is sent once.
    #[test]
    fn two_replaces_within_one_drain_never_run_two_exporters() {
        static SLOT: LifecycleSlot = LifecycleSlot::new();
        let installed = Arc::new(Installed::default());
        let dir = std::env::temp_dir().join(format!("lk-hand-over-chain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let config = |ms| TelemetryConfig {
            storage_dir: Some(dir.to_string_lossy().into_owned()),
            export_timeout_ms: ms,
            ..Default::default()
        };
        let transport = Arc::new(Gated {
            gate: tokio::sync::Semaphore::new(0),
            bodies: Mutex::new(Vec::new()),
        });
        let sent = || transport.bodies.lock().expect("lock").clone();
        runtime().block_on(async {
            let (first, exporter) =
                livekit_telemetry::Telemetry::new(config(20_000), transport.clone());
            configure(&SLOT, &installed, first.clone(), exporter, || ());
            let room = first.begin_scope();
            room.set_server("wss://p.livekit.cloud", &granted_token());
            room.emit_custom("ping", Vec::new());
            tokio::spawn(async move { first.flush().await });
            while sent().is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            let in_flight = sent()[0].clone();
            for _ in 0..2 {
                let (next, exporter) =
                    livekit_telemetry::Telemetry::new(config(1_000), transport.clone());
                next.begin_scope().set_server("wss://p.livekit.cloud", &granted_token());
                configure(&SLOT, &installed, next, exporter, || ());
            }
            // Past both later pipelines' own bounds (2 s each): long enough for a later exporter
            // to replay the cache, were it running beside the first.
            tokio::time::sleep(Duration::from_millis(4_500)).await;
            transport.gate.add_permits(1_000);
            shut_down(&SLOT, &installed).await;
            let copies = sent().iter().filter(|body| **body == in_flight).count();
            assert_eq!(copies, 1, "the batch in flight is sent once");
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A shutdown right after a replace returns only once the replacing exporter, which starts
    /// after the previous pipeline drained for its whole 10 s, has drained too, although the
    /// shutdown's own wait for the replacing pipeline (2 s) ends long before that.
    #[test]
    fn a_shutdown_waits_for_an_exporter_that_started_late() {
        static SLOT: LifecycleSlot = LifecycleSlot::new();
        let installed = Arc::new(Installed::default());
        let gated = Arc::new(Gated {
            gate: tokio::sync::Semaphore::new(0),
            bodies: Mutex::new(Vec::new()),
        });
        let kept = Arc::new(Gated {
            gate: tokio::sync::Semaphore::new(1_000),
            bodies: Mutex::new(Vec::new()),
        });
        let config = |ms| TelemetryConfig { export_timeout_ms: ms, ..Default::default() };
        runtime().block_on(async {
            // The first pipeline's upload never answers: it drains for its whole 10 s timeout.
            let (first, exporter) =
                livekit_telemetry::Telemetry::new(config(10_000), gated.clone());
            configure(&SLOT, &installed, first.clone(), exporter, || ());
            let room = first.begin_scope();
            room.set_server("wss://p.livekit.cloud", &granted_token());
            room.emit_custom("ping", Vec::new());
            tokio::spawn(async move { first.flush().await });
            while gated.bodies.lock().expect("lock").is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            let (second, exporter) = livekit_telemetry::Telemetry::new(config(1_000), kept.clone());
            let room = second.begin_scope();
            room.set_server("wss://p.livekit.cloud", &granted_token());
            room.emit_custom("pong", Vec::new());
            configure(&SLOT, &installed, second, exporter, || ());
            shut_down(&SLOT, &installed).await;
            let sent = kept.bodies.lock().expect("lock").len();
            assert!(sent > 0, "the late exporter drained before the shutdown returned");
        });
    }

    /// Two configures racing, the first paused in the lifecycle right after its install: the
    /// exporters still run one at a time and in install order, so the pipeline installed last is
    /// the one left exporting.
    #[test]
    fn racing_configures_run_exporters_in_install_order() {
        static SLOT: LifecycleSlot = LifecycleSlot::new();
        /// Requests started, requests out right now, and the most out at once.
        #[derive(Default)]
        struct Load {
            started: AtomicU64,
            now: AtomicU64,
            most: AtomicU64,
        }
        /// Counts a request in a load until it ends.
        struct Out(Arc<Load>);
        impl Out {
            fn enter(load: &Arc<Load>) -> Self {
                load.started.fetch_add(1, Ordering::SeqCst);
                let now = load.now.fetch_add(1, Ordering::SeqCst) + 1;
                load.most.fetch_max(now, Ordering::SeqCst);
                Self(load.clone())
            }
        }
        impl Drop for Out {
            fn drop(&mut self) {
                self.0.now.fetch_sub(1, Ordering::SeqCst);
            }
        }
        /// Holds every request until it is cancelled, counted in its pipeline's load and in the
        /// load of both.
        struct Held {
            own: Arc<Load>,
            all: Arc<Load>,
        }
        #[async_trait::async_trait]
        impl TelemetryTransport for Held {
            async fn send(&self, _request: ExportRequest) -> Result<ExportResponse, ExportError> {
                let _out = (Out::enter(&self.own), Out::enter(&self.all));
                std::future::pending().await
            }
        }
        let all = Arc::new(Load::default());
        let installed = Arc::new(Installed::default());
        let pipeline = |name: &str| {
            let own = Arc::new(Load::default());
            let transport = Arc::new(Held { own: own.clone(), all: all.clone() });
            let config = TelemetryConfig { export_timeout_ms: 1_000, ..Default::default() };
            let (telemetry, exporter) = livekit_telemetry::Telemetry::new(config, transport);
            let room = telemetry.begin_scope();
            room.set_server("wss://p.livekit.cloud", &granted_token());
            room.emit_custom(name, Vec::new());
            (telemetry, exporter, own)
        };
        let (a, a_exporter, a_load) = pipeline("a");
        let (b, b_exporter, b_load) = pipeline("b");
        let paused = Arc::new(std::sync::Barrier::new(2));
        let first = std::thread::spawn({
            let (installed, paused) = (installed.clone(), paused.clone());
            move || {
                configure(&SLOT, &installed, a, a_exporter, move || {
                    paused.wait();
                    std::thread::sleep(Duration::from_millis(300));
                });
            }
        });
        paused.wait();
        // While the lifecycle is paused in the first configure, right after its install.
        configure(&SLOT, &installed, b, b_exporter, || ());
        first.join().expect("first configure");
        let started = |load: &Load| load.started.load(Ordering::SeqCst);
        assert!(eventually(|| started(&b_load) > 0), "the pipeline installed last exports");
        let replaced = started(&a_load);
        // Long enough for both exporters to have started, in either order.
        std::thread::sleep(Duration::from_secs(3));
        assert_eq!(all.most.load(Ordering::SeqCst), 1, "one exporter at a time");
        assert_eq!(started(&a_load), replaced, "the replaced pipeline sends nothing afterwards");
        runtime().block_on(shut_down(&SLOT, &installed));
    }

    /// Configure and shutdown take effect in the order they were sent. A configure sent while a
    /// shutdown is under way comes after it: once the shutdown returns, the configure's pipeline
    /// is installed and exporting, and the one before is stopped.
    #[test]
    fn a_configure_sent_during_a_shutdown_comes_after_it() {
        static SLOT: LifecycleSlot = LifecycleSlot::new();
        /// Holds every request until it is cancelled, counting the ones out.
        struct Held(Arc<AtomicU64>);
        /// Takes a request out of the count when it ends.
        struct Out(Arc<AtomicU64>);
        impl Drop for Out {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        #[async_trait::async_trait]
        impl TelemetryTransport for Held {
            async fn send(&self, _request: ExportRequest) -> Result<ExportResponse, ExportError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                let _out = Out(self.0.clone());
                std::future::pending().await
            }
        }
        let installed = Arc::new(Installed::default());
        let (a_out, b_out) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
        let pipeline = |ms, out: &Arc<AtomicU64>| {
            let config = TelemetryConfig { export_timeout_ms: ms, ..Default::default() };
            let transport = Arc::new(Held(out.clone()));
            let (telemetry, exporter) = livekit_telemetry::Telemetry::new(config, transport);
            let room = telemetry.begin_scope();
            room.set_server("wss://p.livekit.cloud", &granted_token());
            room.emit_custom("pending", Vec::new());
            (telemetry, exporter)
        };
        // A's upload never answers: its drain takes its whole 10 s.
        let (a, a_exporter) = pipeline(10_000, &a_out);
        configure(&SLOT, &installed, a, a_exporter, || ());
        assert!(eventually(|| a_out.load(Ordering::SeqCst) == 1), "A exports");
        // The shutdown is held where it uninstalls until the configure below was sent.
        let (entered, uninstalling) = std::sync::mpsc::channel();
        let (release, resume) = std::sync::mpsc::channel::<()>();
        let shutting_down = std::thread::spawn({
            let installed = installed.clone();
            move || {
                runtime().block_on(shutdown_in(&SLOT, move || {
                    entered.send(()).expect("entered");
                    resume.recv().expect("resume");
                    installed.lock().expect("lock").take();
                }))
            }
        });
        uninstalling.recv().expect("the shutdown uninstalls");
        let (b, b_exporter) = pipeline(1_000, &b_out);
        let configuring = std::thread::spawn({
            let installed = installed.clone();
            move || configure(&SLOT, &installed, b, b_exporter, || ())
        });
        wait_until_sent(&SLOT, 3);
        release.send(()).expect("release");
        configuring.join().expect("configure");
        shutting_down.join().expect("shutdown");
        let in_effect = installed.lock().expect("lock").is_some();
        let exporting = eventually(|| b_out.load(Ordering::SeqCst) == 1);
        let stopped = a_out.load(Ordering::SeqCst) == 0;
        runtime().block_on(shut_down(&SLOT, &installed));
        assert!(in_effect, "the configure sent after the shutdown is installed");
        assert!(exporting, "and its pipeline exports");
        assert!(stopped, "the shutdown stopped the pipeline before it");
    }

    /// A shutdown sent last ends the lifecycle and joins it, with its exporter runner and
    /// exporter: no task is left behind, and the next configure starts a fresh lifecycle.
    #[test]
    fn a_last_shutdown_joins_every_task_of_the_lifecycle() {
        let tasks = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("runtime");
        let slot = slot_on(&tasks);
        let before = alive_tasks(&slot);
        let installed = Arc::new(Installed::default());
        for round in 0..2 {
            let transport = Arc::new(Gated {
                gate: tokio::sync::Semaphore::new(1_000),
                bodies: Mutex::new(Vec::new()),
            });
            let config = TelemetryConfig::default();
            let (telemetry, exporter) = livekit_telemetry::Telemetry::new(config, transport);
            configure(&slot, &installed, telemetry, exporter, || ());
            assert!(alive_tasks(&slot) > before, "round {round}: the lifecycle runs");
            runtime().block_on(shut_down(&slot, &installed));
            let joined = eventually(|| alive_tasks(&slot) == before);
            assert!(joined, "round {round}: {} tasks left, {before} before", alive_tasks(&slot));
        }
    }

    /// A shutdown that resumes late, after another shutdown ended its lifecycle and a configure
    /// started a new one, never ends the new lifecycle, although the new one's last command has
    /// the same number as the late shutdown's.
    #[test]
    fn a_late_shutdown_never_ends_a_newer_lifecycle() {
        let tasks = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("runtime");
        let slot = slot_on(&tasks);
        let installed = Arc::new(Installed::default());
        let pipeline = || {
            let config = TelemetryConfig { export_timeout_ms: 1_000, ..Default::default() };
            let transport = Arc::new(Gated {
                gate: tokio::sync::Semaphore::new(1_000),
                bodies: Mutex::new(Vec::new()),
            });
            livekit_telemetry::Telemetry::new(config, transport)
        };
        let (a, a_exporter) = pipeline();
        configure(&slot, &installed, a, a_exporter, || ()); // the first lifecycle's command 1
        runtime().block_on(async {
            // The first half of a shutdown (command 2), which then pauses before it takes the slot.
            let (done, stopped) = oneshot::channel();
            let uninstall = {
                let installed = installed.clone();
                Box::new(move || {
                    installed.lock().expect("lock").take();
                })
            };
            let late =
                slot.send_if_running(Command::Shutdown { uninstall, done }).expect("running");
            stopped.await.expect("its exporter stopped");
            // Meanwhile another shutdown (command 3) ends and joins that lifecycle, and a
            // configure starts a new one: its command 1, then a barrier as its command 2.
            shut_down(&slot, &installed).await;
            assert!(eventually(|| alive_tasks(&slot) == 0), "the first lifecycle is joined");
            let (b, b_exporter) = pipeline();
            configure(&slot, &installed, b, b_exporter, || ());
            barrier(&slot).await;
            // The late shutdown resumes where it paused.
            let taken = slot.take_after(late);
            let ended_the_new_one = taken.is_some();
            if let Some(lifecycle) = taken {
                lifecycle.end().await;
            }
            let still_owned = slot.lock().is_some();
            shut_down(&slot, &installed).await;
            assert!(eventually(|| alive_tasks(&slot) == 0), "every task joined");
            assert!(!ended_the_new_one, "the late shutdown ended the new lifecycle");
            assert!(still_owned, "the new lifecycle keeps its owner");
        });
    }

    /// An exporter that does not stop within its pipeline's shutdown bound is cancelled and
    /// joined: once the stop returns, it is gone rather than left running.
    #[test]
    fn a_stop_past_its_bound_cancels_and_joins_the_exporter() {
        /// Set once the task's future is dropped.
        struct Gone(Arc<AtomicBool>);
        impl Drop for Gone {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let gone = Arc::new(AtomicBool::new(false));
        let config = TelemetryConfig { export_timeout_ms: 1_000, ..Default::default() };
        let transport = Arc::new(Gated {
            gate: tokio::sync::Semaphore::new(0),
            bodies: Mutex::new(Vec::new()),
        });
        runtime().block_on(async {
            // Its exporter never runs, so the shutdown is never answered: the stop waits out the
            // whole bound (2 s), then has to cancel the task that stands in for the exporter.
            let (telemetry, _exporter) = livekit_telemetry::Telemetry::new(config, transport);
            let task = tokio::spawn({
                let gone = Gone(gone.clone());
                async move {
                    let _gone = gone;
                    std::future::pending::<()>().await;
                }
            });
            stop_running(telemetry, task).await;
            assert!(gone.load(Ordering::SeqCst), "the exporter was cancelled and joined");
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

    /// A second opt-out while the first one's purge is blocked does not let a waiter
    /// (`telemetry_flush`/`telemetry_shutdown`) return before that first purge finishes.
    #[test]
    fn a_repeated_opt_out_waits_for_the_first_purge() {
        static SLOT: LifecycleSlot = LifecycleSlot::new();
        let (release, blocked) = oneshot::channel::<()>();
        SLOT.send(Command::Purge(Box::pin(async {
            let _ = blocked.await;
        })));
        SLOT.send(Command::Purge(Box::pin(async {})));
        runtime().block_on(async {
            let early = tokio::time::timeout(Duration::from_millis(200), barrier(&SLOT)).await;
            assert!(early.is_err(), "the first purge is still running");
            release.send(()).expect("first purge waiting");
            tokio::time::timeout(Duration::from_secs(5), barrier(&SLOT))
                .await
                .expect("done once the first purge is");
            shutdown_in(&SLOT, || ()).await;
        });
    }

    /// The opt-out is in effect when `telemetry_disable` returns — no scope, nothing captured — and
    /// once its purge is awaited (`telemetry_shutdown`) the pulled request of a pipeline replaced
    /// while its upload was pending is never handed to the host; `telemetry_is_disabled` turns
    /// `true` for every thread. (The only test here that touches the process-wide pipeline: the
    /// opt-out is one-way.)
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
        // Refused: its generation is revoked.
        room.emit_custom("after".into(), HashMap::new());
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
        let next = runtime()
            .block_on(async { tokio::time::timeout(Duration::from_secs(1), refused.next()).await });
        assert!(matches!(next, Ok(None)), "a refused queue is finished: `next` never waits");
        assert_eq!(std::fs::read_dir(&dir).expect("dir").count(), 0, "the cache is purged");
        let _ = std::fs::remove_dir_all(&dir);
        runtime().block_on(async {
            telemetry_shutdown().await; // awaits the purge
            let served = tokio::time::timeout(Duration::from_millis(300), first.next()).await;
            assert!(!matches!(served, Ok(Some(_))), "withdrawn, never served");
        });
    }
}
