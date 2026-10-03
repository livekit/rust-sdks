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
    collections::{HashMap, VecDeque},
    future::Future,
    io::Write,
    pin::Pin,
    sync::{atomic::AtomicU64, Arc},
    time::Duration,
};

use flate2::{write::GzEncoder, Compression};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep_until, timeout, Instant};

use crate::{
    destination::{Dead, Route, Signal, Target, PROCESS_OWNER},
    event::now_unix_nanos,
    otlp,
    scope::ScopeState,
    stats::{Counters, Snapshot, TelemetryStatus},
    store::Queued,
    telemetry::Shared,
    transport::Verdict,
    AppState, DeviceState, ExportError, ExportRequest, MemoryPressure, NetworkType,
    TelemetryTransport, ThermalState,
};

/// Who a batch belongs to: its session's project (`None` until the session has a server) and
/// the session itself.
#[derive(Debug, PartialEq)]
struct Owner {
    host: Option<String>,
    session: String,
}

/// Local retry backoff: 1 s doubling per consecutive failure up to 60 s, with full jitter
/// (a uniform wait in `[0, backoff]`) so a fleet that failed together does not retry together.
/// Retrying never gives up on a batch: the cache's age and size bound what is kept.
const RETRY_BASE: Duration = Duration::from_secs(1);
const RETRY_CAP: Duration = Duration::from_secs(60);
/// Delays the server asks for (`Retry-After`, `RetryInfo`) are honored even beyond
/// [`RETRY_CAP`]; clamped only to the cache's age limit, past which there is nothing to wait for.
const MAX_SERVER_DELAY: Duration = Duration::from_secs(24 * 60 * 60);
/// A cached batch older than this is dropped (`expired`) — at start and while running.
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// Soft holds (see [`Exporter::hold_reason`]) last at most this long before one batch goes out
/// anyway: the cap that bounds the policy when its signals lie. Hard holds have no such escape.
const MAX_HOLD: Duration = Duration::from_secs(60);
/// While one of these is open the uplink belongs to signaling and ICE/DTLS.
const SENSITIVE_SPANS: &[&str] = &["lk.connect", "lk.reconnect"];
/// RFC 9218 request priority: lowest urgency, not incremental. A hint for HTTP/2+ hops that
/// implement it; the host transport marks the local traffic class (see `TelemetryTransport`).
const PRIORITY: &str = "u=7";

impl Signal {
    fn tag(self) -> char {
        match self {
            Signal::Logs => 'l',
            Signal::Traces => 't',
        }
    }
}

/// A cached batch's id, which is also its ownership record:
/// `<unix ns>-<seq>-<records>-<l|t>-<session>-<project host>`. Sorting ids sorts batches oldest
/// first; the host and the session that produced it pick destination and token at upload time
/// (no host: process-level, sent to the latest project). No token is ever written to disk.
struct BatchId<'a> {
    /// Creation time, unix ns.
    stamp: u64,
    count: u64,
    signal: Signal,
    owner: Option<&'a str>,
    host: Option<&'a str>,
}

impl<'a> BatchId<'a> {
    fn format(signal: Signal, seq: &str, count: u64, owner: &str, host: Option<&str>) -> String {
        let (stamp, tag, host) = (now_unix_nanos(), signal.tag(), host.unwrap_or_default());
        format!("{stamp:020}-{seq}-{count}-{tag}-{owner}-{host}")
    }

    fn parse(id: &'a str) -> Self {
        let mut parts = id.splitn(6, '-');
        let stamp = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        let count = parts.nth(1).and_then(|n| n.parse().ok()).unwrap_or(0);
        let signal = match parts.next() {
            Some("t") => Signal::Traces,
            _ => Signal::Logs,
        };
        let owner = parts.next().filter(|o| !o.is_empty());
        Self { stamp, count, signal, owner, host: parts.next().filter(|h| !h.is_empty()) }
    }

    /// The id of one half of this batch after a 413: same stamp and owner, so it keeps the
    /// original's place in line, `seq` extended with `a`/`b`.
    fn half(id: &str, which: char, count: u64) -> String {
        let mut parts: Vec<&str> = id.splitn(6, '-').collect();
        let seq = format!("{}{which}", parts.get(1).copied().unwrap_or_default());
        let count = count.to_string();
        if parts.len() >= 3 {
            parts[1] = &seq;
            parts[2] = &count;
        }
        parts.join("-")
    }
}

/// The records a cached batch id says it holds.
pub(crate) fn events_in(id: &str) -> u64 {
    BatchId::parse(id).count
}

pub(crate) enum Command {
    Flush(oneshot::Sender<()>),
    Shutdown(oneshot::Sender<()>),
    /// Opt-out: stop without uploading and delete everything held.
    Purge(oneshot::Sender<()>),
}

/// What one request achieved.
enum Outcome {
    Answered(Verdict),
    /// No answer: offline, connection or TLS failure, timeout.
    NoAnswer {
        timed_out: bool,
        reason: String,
    },
}

/// The request on the wire, polled by the actor's loop so commands and deadlines keep being
/// served while it is out.
struct InFlight {
    id: String,
    signal: Signal,
    /// Which destination's pause the answer drives.
    key: String,
    target: Target,
    body: Vec<u8>,
    request: Pin<Box<dyn Future<Output = Outcome> + Send>>,
}

/// One pass over the cache: the batches still to look at and the requests it may make.
#[derive(Default)]
struct Round {
    queue: VecDeque<String>,
    budget: usize,
    attempts: usize,
    /// A batch waited for a token or a destination.
    waiting: bool,
    /// A batch waited for its destination's pause.
    paused: bool,
    /// The one-batch escape of a soft hold: the hold does not stop it.
    escape: bool,
    /// Its requests are charged to the interval's allowance: next to a call, and not a drain,
    /// flush, background pass or escape.
    metered: bool,
    /// The hold policy stopped the pass.
    held: bool,
}

/// A destination's pause after a failure: uploads to it wait, others carry on.
#[derive(Debug, Default)]
struct Pause {
    /// Retry after this (backoff or server delay); `None` once it has passed.
    until: Option<Instant>,
    /// The part the server asked for: honored in full, even while draining.
    server_until: Option<Instant>,
    /// Consecutive failures: the backoff exponent.
    failing: u32,
}

/// Background actor that turns stored events into OTLP requests.
///
/// The role of OTel's `BatchLogRecordProcessor` + OTLP exporter in one place. Every tick it
/// [`enqueue`](Self::enqueue)s: drains the queue and the finished spans, appends an
/// `lk.telemetry.report` when something was dropped or failed since the last one, encodes and
/// gzips one batch per destination project and writes it to the [`BatchCache`](crate::BatchCache)
/// *before* any network is involved; then it [`begin_round`](Self::begin_round)s the cache oldest-first
/// through the [`TelemetryTransport`], one request at a time. What the collector answers decides
/// what happens to the batch ([`Verdict`]); a failure pauses uploads with jittered exponential
/// backoff, a throttle for as long as the collector asked, and collection never stops meanwhile.
///
/// Telemetry must never win over media, so uploads are shaped as well as batched: at most
/// `max_batches_per_upload` per tick while a session may be live, and none at all while the room
/// is connecting or reconnecting or while the device asks for quiet ([`DeviceState::holds_uploads`])
/// — bounded by [`MAX_HOLD`]. Yielding to media on the wire is the transport's job: every request
/// carries `Priority: u=7` (RFC 9218) and a gzipped body.
///
/// The tick period is `flush_interval_ms × cadence factor`: device pressure and a CPU-limited
/// encoder stretch it up to 4×, and entering the background flushes once immediately (the app
/// may be suspended any moment).
///
/// Drive it with `spawn(exporter.run())` on the consumer's runtime. It stops after
/// [`Telemetry::shutdown`](crate::Telemetry::shutdown) or when the last
/// [`Telemetry`](crate::Telemetry) handle is dropped.
pub struct Exporter {
    shared: Arc<Shared>,
    transport: Arc<dyn TelemetryTransport>,
    commands: mpsc::UnboundedReceiver<Command>,
    /// Per-destination pauses after failures, by destination key (the project host).
    pauses: HashMap<String, Pause>,
    /// The upload pass in progress, and its request on the wire.
    round: Option<Round>,
    inflight: Option<InFlight>,
    /// Another pass is due when this one ends (new batches arrived meanwhile).
    rerun: bool,
    /// `flush()` callers, answered when the pass ends.
    flush_waiters: Vec<oneshot::Sender<()>>,
    /// The next pass comes from an explicit `flush()` or entering the background: it has no
    /// per-pass budget.
    flushing: bool,
    /// Requests left in this flush interval (`max_batches_per_upload`, refilled every tick),
    /// charged while a Room is in a call: wake-ups between ticks never add requests beyond it.
    allowance: usize,
    /// Batches done with whose delete failed: never sent again, deletion retried each pass.
    undeletable: std::collections::HashSet<String>,
    /// When this launch wrote each cached batch, on the monotonic clock (see
    /// [`Exporter::expired`]).
    born: HashMap<String, Instant>,
    /// Policy transitions are logged once, not per tick: the hold reason last logged and the
    /// cadence factor last logged.
    hold_logged: Option<&'static str>,
    cadence_logged: u32,
    seq: u64,
    /// Counter values at the last `lk.telemetry.report`.
    last_report: Snapshot,
    /// When the current upload hold began, for the [`MAX_HOLD`] cap.
    held_since: Option<Instant>,
    /// Shutting down: the call is over, so only the device's own holds still apply.
    draining: bool,
    /// Append an `lk.telemetry.report` to the next batch even if nothing went wrong (the
    /// shutdown summary).
    force_report: bool,
}

impl Exporter {
    pub(crate) fn new(
        shared: Arc<Shared>,
        transport: Arc<dyn TelemetryTransport>,
        commands: mpsc::UnboundedReceiver<Command>,
    ) -> Self {
        Self {
            shared,
            transport,
            commands,
            pauses: HashMap::new(),
            round: None,
            inflight: None,
            rerun: false,
            flush_waiters: Vec::new(),
            undeletable: std::collections::HashSet::new(),
            flushing: false,
            allowance: 0,
            born: HashMap::new(),
            hold_logged: None,
            cadence_logged: 1,
            seq: 0,
            last_report: Snapshot::default(),
            held_since: None,
            draining: false,
            force_report: false,
        }
    }

    /// Run until shut down: replay whatever the cache holds, then export on every tick and on
    /// demand. A request on the wire never blocks the loop: commands, ticks and deadlines are
    /// served while it is out.
    pub async fn run(mut self) {
        self.begin_round();
        // Deadlines rather than tickers: `next = last + period`, re-derived every loop, so a
        // cadence change applies to the pending tick in both directions (pressure postpones it,
        // relief brings it forward) and a missed tick never bursts. The first flush is immediate,
        // the first window closes a full period after it opens.
        let mut last_flush = Instant::now() - self.period(self.shared.config.flush_interval_ms);
        let mut last_window = Instant::now();
        loop {
            self.log_cadence();
            let next_flush = last_flush + self.period(self.shared.config.flush_interval_ms);
            let next_window = last_window + self.period(self.shared.config.stats_window_ms);
            let retry = self.retry_deadline();
            let subscribe = self.subscribe_deadline();
            tokio::select! {
                outcome = answer(&mut self.inflight) => self.on_answer(outcome),
                _ = sleep_until(next_flush) => {
                    // A window due at the same moment closes first, so it rides this batch.
                    if Instant::now() >= next_window {
                        self.close_windows();
                        last_window = Instant::now();
                    }
                    // A new interval: a new allowance of requests.
                    self.allowance = self.shared.config.max_batches_per_upload.max(1) as usize;
                    self.export_pending();
                    last_flush = Instant::now();
                }
                _ = sleep_until(next_window) => {
                    self.close_windows();
                    last_window = Instant::now();
                }
                // A pause ran out or a soft hold reached its cap: go without waiting for a tick.
                _ = at(retry) => {
                    self.clear_expired_pauses();
                    self.begin_round();
                }
                _ = at(subscribe) => self.expire_subscribes(),
                // Coalesced wake-ups. Only the queue crossing its threshold or the app going to
                // the background encode a new batch; a lifted route, token or hold re-runs a pass
                // over what is cached (within the interval's allowance); a subscribe or a cadence
                // change only makes the loop re-read its deadlines. So the cadence stays one
                // export per interval.
                _ = self.shared.wake.notified() => {
                    let taken = |flag: &std::sync::atomic::AtomicBool| {
                        flag.swap(false, std::sync::atomic::Ordering::SeqCst)
                    };
                    let background = taken(&self.shared.backgrounded);
                    let overflow = taken(&self.shared.overflowed);
                    let release = taken(&self.shared.released);
                    if background {
                        // The app may be suspended any moment: close the RTC windows and get
                        // everything into the cache and out — the whole cache, like a flush,
                        // whatever is left of the interval's allowance.
                        self.close_windows();
                        self.flushing = true;
                    }
                    if background || overflow {
                        self.export_pending();
                    } else if release {
                        self.begin_round();
                    }
                }
                command = self.commands.recv() => match command {
                    Some(Command::Flush(done)) => {
                        // An explicit flush drains the cache: no per-pass budget, but every hold
                        // and pause still applies.
                        self.flushing = true;
                        self.export_pending();
                        self.answer_when_done(done);
                    }

                    Some(Command::Shutdown(done)) => {
                        self.drain().await;
                        let _ = done.send(());
                        return;
                    }
                    Some(Command::Purge(done)) => {
                        // Cancel the request on the wire: its answer must not bring anything back.
                        self.inflight = None;
                        self.round = None;
                        self.shared.clear();
                        self.answer_flushes();
                        let _ = done.send(());
                        return;
                    }
                    // Every `Telemetry` handle is gone.
                    None => {
                        self.drain().await;
                        return;
                    }
                },
            }
        }
    }

    /// `base_ms × cadence factor`.
    fn period(&self, base_ms: u64) -> Duration {
        Duration::from_millis(base_ms.max(1)) * self.cadence_factor()
    }

    fn device(&self) -> DeviceState {
        self.shared.device.lock().unwrap_or_else(|e| e.into_inner()).unwrap_or_default()
    }

    fn cadence_factor(&self) -> u32 {
        self.shared.cadence_factor()
    }

    fn cpu_limited(&self) -> bool {
        self.shared.windows.lock().unwrap_or_else(|e| e.into_inner()).cpu_limited()
    }

    /// Whether a Room is in a call: it has a server and has not disconnected.
    fn in_call(&self) -> bool {
        let scopes = self.shared.scopes.lock().unwrap_or_else(|e| e.into_inner());
        scopes.iter().any(|s| s.upgrade().is_some_and(|s| s.in_call()))
    }

    /// Every live session's earliest subscribe deadline.
    fn subscribe_deadline(&self) -> Option<Instant> {
        let scopes = self.shared.scopes.lock().unwrap_or_else(|e| e.into_inner());
        scopes.iter().filter_map(|s| s.upgrade()?.subscribe_deadline()).min()
    }

    fn expire_subscribes(&self) {
        let scopes: Vec<Arc<ScopeState>> = {
            let scopes = self.shared.scopes.lock().unwrap_or_else(|e| e.into_inner());
            scopes.iter().filter_map(|s| s.upgrade()).collect()
        };
        for scope in scopes {
            scope.expire_subscribes();
        }
    }

    /// One debug line per cadence change, naming what stretched it.
    fn log_cadence(&mut self) {
        let factor = self.cadence_factor();
        if factor == self.cadence_logged {
            return;
        }
        self.cadence_logged = factor;
        let device = self.device();
        let mut why = Vec::new();
        if !matches!(device.thermal, ThermalState::Unknown | ThermalState::Nominal) {
            why.push(format!("thermal {:?}", device.thermal).to_lowercase());
        }
        if device.memory != MemoryPressure::Normal {
            why.push(format!("memory {:?}", device.memory).to_lowercase());
        }
        if device.low_power_mode == Some(true) {
            why.push("low power mode".to_string());
        }
        if device.app_state == AppState::Background {
            why.push("background".to_string());
        }
        if self.cpu_limited() {
            why.push("encoder cpu-limited".to_string());
        }
        log::debug!(
            "cadence ×{factor}, flush every {}s ({})",
            self.period(self.shared.config.flush_interval_ms).as_secs(),
            if why.is_empty() { "pressure over".to_string() } else { why.join(", ") }
        );
    }

    /// Last chance: everything queued into the cache and out, within `export_timeout_ms`.
    /// Ignores the failure backoff, the batch budget and the session holds (the call is over);
    /// respects server-directed delays and the device's own holds. At the deadline the request
    /// on the wire is cancelled and the actor stops: its batch stays cached for the next launch.
    async fn drain(&mut self) {
        let deadline =
            Instant::now() + Duration::from_millis(self.shared.config.export_timeout_ms.max(1));
        self.draining = true;
        self.force_report = true;
        // Pending subscribes end now and ride the last batch; nothing keeps a span afterwards.
        self.shared.end_pending_subscribes(true);
        for pause in self.pauses.values_mut() {
            pause.until = pause.server_until.filter(|t| Instant::now() < *t);
        }
        self.close_windows();
        self.export_pending();
        let mut commands_open = true;
        while self.round.is_some() || self.inflight.is_some() {
            tokio::select! {
                outcome = answer(&mut self.inflight) => self.on_answer(outcome),
                _ = sleep_until(deadline) => {
                    log::debug!("shutdown deadline: cancelling the upload in flight");
                    self.inflight = None;
                    self.round = None;
                    break;
                }
                // The opt-out reaches a draining generation too: cancel, clear, confirm.
                command = self.commands.recv(), if commands_open => match command {
                    Some(Command::Purge(done)) => {
                        self.inflight = None;
                        self.round = None;
                        self.shared.clear();
                        let _ = done.send(());
                        break;
                    }
                    Some(Command::Flush(done)) => self.answer_when_done(done),
                    Some(Command::Shutdown(done)) => {
                        let _ = done.send(());
                    }
                    None => commands_open = false,
                },
            }
            // A pass that ended with a request left to make (a split, the rerun) goes on.
            if self.inflight.is_none() && self.round.is_some() {
                self.advance();
            }
        }
        self.answer_flushes();
        let left = self.shared.cache.pending().len();
        if left > 0 {
            log::debug!("{left} batches still cached at shutdown (replayed next start)");
        }
    }

    /// Turn every open RTC stats window into its `lk.rtc.stats.sample` event. Windows bypass the
    /// flood guard: they are the pipeline's own, bounded output.
    fn close_windows(&mut self) {
        let events = self.shared.windows.lock().unwrap_or_else(|e| e.into_inner()).close();
        for event in events {
            self.shared.store.push(event);
        }
    }

    fn export_pending(&mut self) {
        self.enqueue();
        self.begin_round();
    }

    /// Split records by owner — the project their session routes to, and the session — so each
    /// batch has exactly one destination and one token. Records whose project receives nothing
    /// are dropped here, counted as `disabled`.
    fn by_owner<T>(
        &self,
        items: Vec<T>,
        owner_of: impl Fn(&T) -> (&ScopeState, Option<&String>),
    ) -> Vec<(Owner, Vec<T>)> {
        let destinations = self.destinations();
        let mut groups: Vec<(Owner, Vec<T>)> = Vec::new();
        for item in items {
            let (session, route) = owner_of(&item);
            let process = session.trace_id == self.shared.process.trace_id;
            let session = if process { PROCESS_OWNER.to_owned() } else { session.hex() };
            // The project captured with the record, never the session's current one — or, for a
            // Room record captured before its first server, that Room's first project, so the
            // cached batch names its project and replays after a restart.
            let host = match route {
                Some(route) => Some(route.clone()),
                None if !process => destinations.first_project(&session),
                None => None,
            };
            let owner = Owner { host, session };
            if !destinations.alive(owner.host.as_deref()) {
                Counters::add(&self.shared.counters.disabled, 1);
                continue;
            }
            match groups.iter_mut().find(|(o, _)| *o == owner) {
                Some((_, group)) => group.push(item),
                None => groups.push((owner, vec![item])),
            }
        }
        groups
    }

    /// Encode everything queued — log records and finished spans — into the cache. No network.
    fn enqueue(&mut self) {
        self.enqueue_spans();
        let config = &self.shared.config;
        let (max, max_bytes) = (
            config.max_batch_size.max(1) as usize,
            usize::try_from(config.max_batch_bytes.max(1)).unwrap_or(usize::MAX),
        );
        // One report per pass at most: a report that is itself dropped (oversized) or evicts
        // another batch must not make the next report due in the same pass, or with
        // `max_batch_size` 1 it would take the only place forever and no record would move.
        let mut reported = false;
        loop {
            // Self-telemetry rides along with real data: never its own request, never its own
            // cadence, and only when there is something to report — plus once at shutdown. It
            // takes one of the batch's `max_batch_size` places.
            let now = self.shared.counters.snapshot();
            let delta = now.since(&self.last_report);
            let report_due = !reported && (delta.has_problems() || self.force_report);
            let room = if report_due { max - 1 } else { max };
            let mut batch = self.shared.store.drain(room, max_bytes);
            // Real data to ride along with (with `max_batch_size` 1 the report goes first, alone);
            // the shutdown summary goes out even with nothing else queued.
            let data = !batch.is_empty() || (room == 0 && !self.shared.store.is_empty());
            if !data && !self.force_report {
                return;
            }
            if report_due {
                let cached = self.shared.cache.pending().len() as u64;
                // The host's console gets the cumulative line through the FFI log path.
                log::debug!("{}", crate::stats::TelemetryStats::new(now, cached, self.status()));
                let report = delta.report(cached);
                batch.push(Queued::new(report, self.shared.process.clone()));
                self.last_report = now;
                self.force_report = false;
                reported = true;
            }
            let global = self.shared.global.lock().unwrap_or_else(|e| e.into_inner()).clone();
            for (owner, records) in self.by_owner(batch, |q| (&q.session, q.route.as_ref())) {
                let count = records.len() as u64;
                let body = otlp::encode_logs(&self.shared.config.resource, &global, records);
                self.push_batch(Signal::Logs, &owner, count, &body);
            }
        }
    }

    /// Finished spans travel as their own batches on the traces signal: every one of them, so a
    /// burst larger than a batch (or a shutdown) leaves nothing behind in memory.
    fn enqueue_spans(&mut self) {
        loop {
            let (spans, dropped) = {
                let mut registry = self.shared.spans.lock().unwrap_or_else(|e| e.into_inner());
                (
                    registry.drain(
                        self.shared.config.max_batch_size.max(1) as usize,
                        usize::try_from(self.shared.config.max_batch_bytes.max(1))
                            .unwrap_or(usize::MAX),
                    ),
                    registry.take_dropped(),
                )
            };
            Counters::add(&self.shared.counters.queue_full, dropped);
            if spans.is_empty() {
                return;
            }
            let global = self.shared.global.lock().unwrap_or_else(|e| e.into_inner()).clone();
            for (owner, spans) in self.by_owner(spans, |s| (&s.session, s.route.as_ref())) {
                let count = spans.len() as u64;
                let body = otlp::encode_spans(&self.shared.config.resource, &global, spans);
                self.push_batch(Signal::Traces, &owner, count, &body);
            }
        }
    }

    /// Gzip the encoded batch and cache it. Compressed at rest as well as on the wire: the cache
    /// holds 5–10× more, the disk write shrinks, and a replay costs no CPU.
    ///
    /// The record-size estimates that cut batches cannot see what encoding adds (session and
    /// pipeline attributes, the self-report), so the real encoded size is checked here: a batch
    /// over `max_batch_bytes` is halved until it fits, or is a single record.
    fn push_batch(&mut self, signal: Signal, owner: &Owner, count: u64, body: &[u8]) {
        let limit =
            usize::try_from(self.shared.config.max_batch_bytes.max(1)).unwrap_or(usize::MAX);
        if body.len() > limit {
            let halves = match signal {
                Signal::Logs => otlp::split_logs(body),
                Signal::Traces => otlp::split_spans(body),
            };
            if let Some([(first, first_count), (second, second_count)]) = halves {
                self.push_batch(signal, owner, first_count, &first);
                self.push_batch(signal, owner, second_count, &second);
            } else {
                // One record larger than a request may be: never cached, never sent.
                log::warn!("a record of {} bytes exceeds max_batch_bytes; dropped", body.len());
                Counters::add(&self.shared.counters.oversized, count);
            }
            return;
        }
        let body = gzip(body);
        self.seq += 1;
        let seq = format!("{:06}", self.seq);
        let id = BatchId::format(signal, &seq, count, &owner.session, owner.host.as_deref());
        self.store_batch(id, &body);
    }

    /// Write one gzipped batch to the cache — unless the app opted out meanwhile.
    fn store_batch(&mut self, id: String, body: &[u8]) {
        if self.shared.revoked() {
            return;
        }
        self.born.insert(id.clone(), Instant::now());
        match self.shared.cache.push(&id, body) {
            Ok(evicted) => self.count_evicted(&evicted),
            Err(err) => {
                log::debug!("could not cache {id}: {err}");
                Counters::add(&self.shared.counters.cache_error, events_in(&id));
                self.born.remove(&id);
            }
        }
    }

    /// Older batches pushed out by the cache's bounds: lost, but counted. Losing them mid-hold is
    /// its own answer — the collector held us off longer than the cache could carry — so it is
    /// counted apart from an ordinary overflow.
    fn count_evicted(&mut self, evicted: &[String]) {
        let now = Instant::now();
        let counters = self.shared.counters.clone();
        for id in evicted {
            // `throttled` only when the evicted batch's own destination is under a server pause —
            // for a host-less batch (process-level, pre-connect), the project it routes to.
            let batch = BatchId::parse(id);
            let host = match self.destinations().route(batch.host, batch.owner) {
                Route::Send(target) => target.project.unwrap_or_default(),
                _ => batch.host.unwrap_or_default().to_owned(),
            };
            let held =
                self.pauses.get(&host).is_some_and(|p| p.server_until.is_some_and(|t| now < t));
            let counter = if held { &counters.throttled } else { &counters.cache_full };
            Counters::add(counter, events_in(id));
            self.born.remove(id);
        }
    }

    /// Why uploads should wait right now, if they should. Data keeps flowing into the cache
    /// meanwhile — write-ahead caching is what makes holding free.
    fn hold_reason(&self) -> Option<&'static str> {
        if self.device().holds_uploads() {
            return Some("device asks for quiet");
        }
        if self.draining {
            return None;
        }
        if self.shared.spans.lock().unwrap_or_else(|e| e.into_inner()).any_open(SENSITIVE_SPANS) {
            return Some("connecting");
        }
        None
    }

    fn status(&self) -> TelemetryStatus {
        *self.shared.status.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Record the policy state; true when it changed (log once, not per tick).
    fn set_status(&self, status: TelemetryStatus) -> bool {
        let mut current = self.shared.status.lock().unwrap_or_else(|e| e.into_inner());
        let changed = *current != status;
        *current = status;
        changed
    }

    /// Log a hold transition once.
    fn log_hold(&mut self, reason: Option<&'static str>, backlog: usize) {
        match (self.hold_logged, reason) {
            (None, Some(now)) => log::debug!("held ({now}), backlog {backlog}"),
            (Some(was), Some(now)) if was != now => {
                log::debug!("held ({now}, was {was}), backlog {backlog}")
            }
            (Some(was), None) => match self.held_since {
                Some(since) => log::debug!(
                    "resumed after {}s ({was}), backlog {backlog}",
                    since.elapsed().as_secs()
                ),
                None => log::debug!("resumed ({was}), backlog {backlog}"),
            },
            _ => {}
        }
        self.hold_logged = reason;
    }

    /// How many batches this call may send: none while held — a hard hold (offline) for as long
    /// as it lasts, a soft one up to [`MAX_HOLD`], then one — otherwise what is left of the
    /// interval's allowance when `metered`, everything when not.
    fn budget(&mut self, backlog: usize, metered: bool) -> (usize, bool) {
        if self.offline() {
            self.log_hold(Some("offline"), backlog);
            return (0, false);
        }
        let reason = self.hold_reason();
        self.log_hold(reason, backlog);
        let Some(reason) = reason else {
            self.held_since = None;
            return (if metered { self.allowance } else { usize::MAX }, false);
        };
        let since = *self.held_since.get_or_insert_with(Instant::now);
        if since.elapsed() < MAX_HOLD {
            log::trace!("holding {backlog} batches: {reason}");
            return (0, false);
        }
        log::warn!("held {}s ({reason}): sending one batch anyway", MAX_HOLD.as_secs());
        // Held long enough: one batch goes out, then the hold starts over.
        self.held_since = Some(Instant::now());
        Counters::add(&self.shared.counters.hold_cap_hits, 1);
        (1, true)
    }

    /// A hard hold: nothing is attempted while the device is offline.
    fn offline(&self) -> bool {
        self.device().network == NetworkType::Unavailable
    }

    /// Whether the pass may make its next request: the hold policy is evaluated before every
    /// request, not once per pass. A hard hold stops the pass; a soft hold that began meanwhile
    /// stops it too — unless this pass is the hold's one-batch escape (or a shutdown drain).
    fn may_send(&mut self) -> bool {
        // The opt-out stops a pass mid-way (a second guard: `advance` checks it as well).
        if self.offline() || self.shared.revoked() {
            return false;
        }
        if self.draining || self.round.as_ref().is_some_and(|r| r.escape) {
            return true;
        }
        if self.hold_reason().is_some() {
            self.held_since.get_or_insert_with(Instant::now);
            return false;
        }
        true
    }

    /// Whether a cached batch is past [`MAX_AGE`]. Wall-clock age decides for batches from a
    /// previous launch; for this launch's own the monotonic clock must agree, so a clock set
    /// forward cannot expire a fresh backlog (and one set back only delays expiry).
    fn expired(&self, id: &str) -> bool {
        let wall = Duration::from_nanos(now_unix_nanos().saturating_sub(BatchId::parse(id).stamp));
        wall > MAX_AGE && self.born.get(id).is_none_or(|born| born.elapsed() > MAX_AGE)
    }

    /// When the loop must wake to upload without a tick: the earliest pause to run out, or a
    /// soft hold reaching its cap while batches wait.
    fn retry_deadline(&self) -> Option<Instant> {
        let pause = self.pauses.values().filter_map(|p| p.until).min();
        // A hard hold suppresses the soft hold's cap: waking for it could send nothing. It comes
        // back when the hard hold lifts (a device change wakes the loop).
        let hold = self
            .held_since
            .filter(|_| self.round.is_none() && !self.offline())
            .filter(|_| !self.shared.cache.pending().is_empty())
            .map(|since| since + MAX_HOLD);
        pause.into_iter().chain(hold).min()
    }

    fn clear_expired_pauses(&mut self) {
        let now = Instant::now();
        for pause in self.pauses.values_mut() {
            pause.until = pause.until.filter(|t| now < *t);
            pause.server_until = pause.server_until.filter(|t| now < *t);
        }
    }

    /// Answer a `flush()` once the pass it started is over.
    fn answer_when_done(&mut self, done: oneshot::Sender<()>) {
        if self.round.is_some() || self.inflight.is_some() {
            // Callers that stopped waiting are not kept.
            self.flush_waiters.retain(|waiter| !waiter.is_closed());
            self.flush_waiters.push(done);
        } else {
            let _ = done.send(());
        }
    }

    fn answer_flushes(&mut self) {
        for done in self.flush_waiters.drain(..) {
            let _ = done.send(());
        }
    }

    /// Start a pass over the cache: oldest first, within this tick's budget, one request at a
    /// time. A pass already running picks the new batches up in a second pass.
    fn begin_round(&mut self) {
        #[cfg(test)]
        self.shared.passes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if self.shared.revoked() {
            return;
        }
        if self.round.is_some() || self.inflight.is_some() {
            self.rerun = true;
            return;
        }
        // An explicit flush applies to this pass only, whatever it finds.
        let flushing = std::mem::take(&mut self.flushing);
        let counters = self.shared.counters.clone();
        self.retry_deletes();
        self.bind_pre_connect();
        for id in self.shared.cache.pending() {
            if self.expired(&id) {
                self.forget(&id, Some(&counters.expired));
            }
        }
        self.retire_credentials();
        if self.nobody_listens() {
            self.purge_dead();
            self.set_status(TelemetryStatus::Off);
            return self.answer_flushes();
        }
        let pending = self.shared.cache.pending();
        if pending.is_empty() {
            self.held_since = None;
            self.set_status(TelemetryStatus::Ok);
            return self.answer_flushes();
        }
        // Only next to a call is a pass charged to the interval's allowance.
        let metered = !(self.draining || flushing) && self.in_call();
        let (budget, escape) = self.budget(pending.len(), metered);
        if budget == 0 {
            if self.held_since.is_some() || self.offline() {
                self.set_status(TelemetryStatus::Held);
            }
            return self.answer_flushes();
        }
        let metered = metered && !escape;
        self.round =
            Some(Round { queue: pending.into(), budget, escape, metered, ..Round::default() });
        self.advance();
    }

    /// Take the pass to its next request, or to its end.
    fn advance(&mut self) {
        if self.inflight.is_some() || self.shared.revoked() {
            return;
        }
        let counters = self.shared.counters.clone();
        loop {
            let next = match self.round.as_mut() {
                None => return,
                Some(round) if round.attempts >= round.budget => None,
                Some(round) => round.queue.pop_front(),
            };
            let Some(id) = next else { return self.finish_round() };
            if self.undeletable.contains(&id) {
                continue;
            }
            if !self.may_send() {
                self.mark(|round| round.held = true);
                return self.finish_round();
            }
            let (route, signal) = {
                let batch = BatchId::parse(&id);
                (self.destinations().route(batch.host, batch.owner), batch.signal)
            };
            let target = match route {
                Route::Send(target) => target,
                // A hard hold: no token that may be used (missing, expired, grant-less, refused).
                Route::Wait => {
                    self.mark(|round| round.waiting = true);
                    continue;
                }
                Route::Drop => {
                    self.forget(&id, Some(&counters.disabled));
                    continue;
                }
            };
            // The project the request goes to — and nothing else — is what its answer is about.
            let key = target.project.clone().unwrap_or_default();
            if self.pauses.get(&key).is_some_and(|p| p.until.is_some_and(|t| Instant::now() < t)) {
                self.mark(|round| round.paused = true);
                continue;
            }
            let body = match self.shared.cache.read(&id) {
                Ok(body) if intact(&body) => body,
                Ok(_) => {
                    log::debug!("cached batch {id} is corrupt; dropped");
                    self.forget(&id, Some(&counters.corrupt));
                    continue;
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    self.born.remove(&id);
                    continue;
                }
                // There but unreadable right now (file protection on a locked device, a
                // permission): not corrupt — it is left alone and tried again later.
                Err(err) => {
                    log::debug!("cached batch {id} is inaccessible ({err}); trying later");
                    self.mark(|round| round.waiting = true);
                    continue;
                }
            };
            self.mark(|round| round.attempts += 1);
            if self.round.as_ref().is_some_and(|r| r.metered) {
                self.allowance = self.allowance.saturating_sub(1);
            }
            let request = self.request(&body, signal, &target);
            self.inflight = Some(InFlight { id, signal, key, target, body, request });
            return;
        }
    }

    fn mark(&mut self, f: impl FnOnce(&mut Round)) {
        if let Some(round) = self.round.as_mut() {
            f(round);
        }
    }

    /// The pass is over: report where uploads stand, then start the next pass if one is due.
    fn finish_round(&mut self) {
        let Some(round) = self.round.take() else { return };
        let now = Instant::now();
        let paused = |server: bool| {
            self.pauses.values().any(|p| {
                let until = if server { p.server_until } else { p.until };
                until.is_some_and(|t| now < t)
            })
        };
        let status = if paused(true) {
            TelemetryStatus::Throttled
        } else if paused(false) {
            TelemetryStatus::Paused
        } else if round.held {
            TelemetryStatus::Held
        } else if round.waiting && round.attempts == 0 {
            TelemetryStatus::Waiting
        } else if self.held_since.is_some() {
            TelemetryStatus::Held
        } else {
            TelemetryStatus::Ok
        };
        if self.set_status(status) && status == TelemetryStatus::Waiting {
            log::debug!("waiting for a destination or a usable token");
        }
        if std::mem::take(&mut self.rerun) {
            self.begin_round();
        }
        if self.round.is_none() && self.inflight.is_none() {
            self.answer_flushes();
        }
    }

    /// What the collector answered to the request on the wire, acted on; then the pass goes on.
    fn on_answer(&mut self, outcome: Outcome) {
        let Some(InFlight { id, signal, key, target, body, .. }) = self.inflight.take() else {
            return;
        };
        // The app opted out while the request was out: whatever came back, nothing is written,
        // retried or rescheduled.
        if self.shared.revoked() {
            self.round = None;
            return;
        }
        let counters = self.shared.counters.clone();
        match outcome {
            Outcome::Answered(Verdict::Accepted { rejected, reason }) => {
                self.forget(&id, None);
                Counters::add(&counters.uploads_sent, 1);
                Counters::add(&counters.upload_bytes, body.len() as u64);
                // The batch's other records were accepted; the refused ones are counted, never
                // retried (OTLP partial success).
                if rejected > 0 {
                    log::warn!("collector refused {rejected} records: {reason}");
                    Counters::add(&counters.rejected, rejected);
                }
                self.recovered(&key);
            }
            Outcome::Answered(Verdict::TooLarge) => {
                // Split in two; the halves go at once, down to single records — each one a
                // request of its own against this pass's budget.
                match self.split(&id, &body, signal) {
                    Ok(halves) if halves.is_empty() => {
                        log::warn!("a single record is larger than the collector accepts; dropped");
                        self.forget(&id, Some(&counters.oversized));
                    }
                    Ok(halves) => {
                        if let Some(round) = self.round.as_mut() {
                            for half in halves.into_iter().rev() {
                                round.queue.push_front(half);
                            }
                        }
                    }
                    Err(err) => {
                        // The batch stays whole where it was; try the split again later.
                        self.back_off(&key, None, &format!("cannot store the split batch: {err}"));
                    }
                }
            }
            Outcome::Answered(Verdict::Rejected(reason)) => {
                log::warn!("batch rejected by the collector, data lost: {reason}");
                self.forget(&id, Some(&counters.rejected));
                self.recovered(&key);
            }
            Outcome::Answered(
                verdict @ (Verdict::Unauthorized(_) | Verdict::NotFound | Verdict::Disabled),
            ) if self.has_override() => {
                log::warn!("the collector at {} refused a batch: {verdict:?}", target.logs);
                self.forget(&id, Some(&counters.rejected));
            }
            Outcome::Answered(Verdict::Unauthorized(reason)) => {
                // A credential problem, not a data problem: this token is not used again and the
                // batch waits for the next one.
                log::debug!("upload unauthorized: {reason}");
                Counters::add(&counters.auth_denied, 1);
                if let Some(token) = &target.token {
                    self.destinations().refuse(token);
                }
                if let Some(round) = self.round.as_mut() {
                    round.waiting = true;
                }
            }
            Outcome::Answered(Verdict::NotFound) => {
                self.kill(target.project.as_deref(), Dead::NotFound)
            }
            Outcome::Answered(Verdict::Disabled) => {
                self.kill(target.project.as_deref(), Dead::Disabled)
            }
            Outcome::Answered(Verdict::Throttled { delay_ms, reason }) => {
                Counters::add(&counters.upload_failures, 1);
                self.back_off(&key, Some(Duration::from_millis(delay_ms)), &reason);
            }
            Outcome::Answered(Verdict::Retry { delay_ms, reason }) => {
                Counters::add(&counters.upload_failures, 1);
                self.back_off(&key, delay_ms.map(Duration::from_millis), &reason);
            }
            Outcome::NoAnswer { timed_out, reason } => {
                let counter =
                    if timed_out { &counters.upload_timeouts } else { &counters.upload_failures };
                Counters::add(counter, 1);
                self.back_off(&key, None, &reason);
            }
        }
        self.advance();
    }

    /// 413: re-encode the batch as two halves and replace it in the cache transactionally —
    /// both halves committed where the batch was (on disk stays on disk) before it goes, nothing
    /// evicted in between. `Ok` holds the halves' ids, oldest first, or none for a single record
    /// (it cannot shrink); `Err` means the cache could not take the halves and the batch is
    /// still there, whole.
    fn split(&mut self, id: &str, body: &[u8], signal: Signal) -> std::io::Result<Vec<String>> {
        let Some(encoded) = gunzip(body) else { return Ok(Vec::new()) };
        let halves = match signal {
            Signal::Logs => otlp::split_logs(&encoded),
            Signal::Traces => otlp::split_spans(&encoded),
        };
        let Some([(first, first_count), (second, second_count)]) = halves else {
            return Ok(Vec::new());
        };
        let new = [
            (BatchId::half(id, 'a', first_count), gzip(&first)),
            (BatchId::half(id, 'b', second_count), gzip(&second)),
        ];
        let evicted = self.shared.cache.replace(id, &new)?;
        self.count_evicted(&evicted);
        self.born.remove(id);
        let now = Instant::now();
        for (half, _) in &new {
            self.born.insert(half.clone(), now);
        }
        log::debug!(
            "413: split a batch of {} into {first_count} + {second_count}",
            first_count + second_count
        );
        Ok(new.into_iter().map(|(half, _)| half).collect())
    }

    /// A Room batch cached before the Room had a server names no project; once the Room's first
    /// project is known it is rewritten under an id that names it (a journaled replace, as
    /// crash-safe as a push), so it replays after a restart, when this launch's Room → project
    /// map is gone. Recovery rolls an interrupted rewrite forward once its journal is durable
    /// ([`FileCache`](crate::cache::FileCache)); a crash before that leaves the batch unbound.
    fn bind_pre_connect(&mut self) {
        for id in self.shared.cache.pending() {
            let batch = BatchId::parse(&id);
            let (None, Some(owner)) = (batch.host, batch.owner) else { continue };
            // Accepted already, its delete pending: a new id would be sent again.
            if owner == PROCESS_OWNER || self.undeletable.contains(&id) {
                continue;
            }
            let Some(project) = self.destinations().first_project(owner) else { continue };
            let Ok(body) = self.shared.cache.read(&id) else { continue };
            // A host-less id ends with the empty host: appending one is the bound id.
            let bound = format!("{id}{project}");
            if let Ok(evicted) = self.shared.cache.replace(&id, &[(bound.clone(), body)]) {
                if let Some(born) = self.born.remove(&id) {
                    self.born.insert(bound, born);
                }
                self.count_evicted(&evicted);
            }
        }
    }

    /// Keep credentials only for Rooms still alive or with batches still cached.
    fn retire_credentials(&self) {
        let live: HashMap<String, Option<String>> = {
            let scopes = self.shared.scopes.lock().unwrap_or_else(|e| e.into_inner());
            scopes.iter().filter_map(|s| s.upgrade()).map(|s| (s.hex(), s.route())).collect()
        };
        let backlog = self
            .shared
            .cache
            .pending()
            .iter()
            .filter_map(|id| {
                let batch = BatchId::parse(id);
                Some((batch.host.map(str::to_owned), batch.owner?.to_owned()))
            })
            .collect();
        self.destinations().retain_owners(&live, &backlog);
    }

    fn destinations(&self) -> std::sync::MutexGuard<'_, crate::destination::Destinations> {
        self.shared.destinations.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn has_override(&self) -> bool {
        self.destinations().has_override()
    }

    /// Every project the process talked to receives nothing: telemetry is off.
    fn nobody_listens(&self) -> bool {
        matches!(self.destinations().route(None, None), Route::Drop)
    }

    /// The project on `host` receives nothing more: its cached batches go now.
    fn kill(&mut self, host: Option<&str>, why: Dead) {
        if let Some(host) = host {
            self.destinations().kill(host, why);
        }
        self.purge_dead();
    }

    /// Delete every cached batch whose project receives nothing, counted as `disabled`.
    fn purge_dead(&mut self) {
        let counters = self.shared.counters.clone();
        for id in self.shared.cache.pending() {
            let batch = BatchId::parse(&id);
            if self.destinations().route(batch.host, batch.owner) == Route::Drop {
                self.forget(&id, Some(&counters.disabled));
            }
        }
    }

    /// Remove a cached batch; when it was lost, count its records against that reason. A
    /// delete that fails leaves the batch pending deletion: never uploaded again, deleted as soon
    /// as the storage lets us (a restart before then may send it once more).
    fn forget(&mut self, id: &str, lost: Option<&AtomicU64>) {
        if let Err(err) = self.shared.cache.remove(id) {
            log::warn!("cannot delete cached batch {id} ({err}); it will not be sent again");
            self.undeletable.insert(id.to_owned());
        }
        self.born.remove(id);
        if let Some(counter) = lost {
            Counters::add(counter, events_in(id));
        }
    }

    /// Try the deletes that failed before.
    fn retry_deletes(&mut self) {
        let cache = self.shared.cache.clone();
        self.undeletable.retain(|id| cache.remove(id).is_err());
    }

    fn recovered(&mut self, key: &str) {
        if let Some(pause) = self.pauses.remove(key) {
            log::info!(
                "uploads recovered after {} failures, backlog {}",
                pause.failing,
                self.shared.cache.pending().len()
            );
        }
    }

    /// Pause uploads to one destination after a failure — the others carry on: for the delay the
    /// server named, honored in full (neither shutdown nor a hold's escape hatch cuts it short),
    /// else for a jittered exponential backoff.
    fn back_off(&mut self, key: &str, server_delay: Option<Duration>, reason: &str) {
        let pause = self.pauses.entry(key.to_owned()).or_default();
        pause.failing += 1;
        let wait = match server_delay {
            Some(delay) => {
                let delay = delay.min(MAX_SERVER_DELAY);
                pause.server_until = Some(Instant::now() + delay);
                delay
            }
            None => backoff(pause.failing),
        };
        pause.until = Some(Instant::now() + wait);
        let failing = pause.failing;
        let backlog = self.shared.cache.pending().len();
        let wait = wait.as_secs_f64();
        if failing == 1 {
            log::warn!("upload failed: {reason}; retrying in {wait:.1}s, backlog {backlog}");
        } else {
            log::debug!(
                "upload failed ({failing} in a row): {reason}; retrying in {wait:.1}s, backlog {backlog}"
            );
        }
    }

    /// One request, bounded by `export_timeout_ms`, as a future the loop polls. Never retried
    /// here: the answer decides.
    fn request(
        &self,
        body: &[u8],
        signal: Signal,
        target: &Target,
    ) -> Pin<Box<dyn Future<Output = Outcome> + Send>> {
        let mut headers = HashMap::from([
            ("Content-Type".to_owned(), otlp::CONTENT_TYPE.to_owned()),
            ("Content-Encoding".to_owned(), "gzip".to_owned()),
            ("Priority".to_owned(), PRIORITY.to_owned()),
        ]);
        if let Some(token) = &target.token {
            headers.insert("Authorization".to_owned(), format!("Bearer {token}"));
        }
        let request =
            ExportRequest { url: target.url(signal).to_owned(), headers, body: body.to_vec() };
        let bound = Duration::from_millis(self.shared.config.export_timeout_ms.max(1));
        let transport = self.transport.clone();
        Box::pin(async move {
            match timeout(bound, transport.send(request)).await {
                Ok(Ok(response)) => Outcome::Answered(Verdict::of(&response)),
                Ok(Err(ExportError::Retryable { reason, retry_after_ms: Some(ms) })) => {
                    Outcome::Answered(Verdict::Throttled { delay_ms: ms, reason })
                }
                Ok(Err(ExportError::Retryable { reason, retry_after_ms: None })) => {
                    Outcome::NoAnswer { timed_out: false, reason }
                }
                Ok(Err(ExportError::Rejected { reason })) => {
                    Outcome::Answered(Verdict::Rejected(reason))
                }
                Ok(Err(ExportError::Disabled)) => Outcome::Answered(Verdict::Disabled),
                Err(_) => Outcome::NoAnswer { timed_out: true, reason: "timed out".to_owned() },
            }
        })
    }
}

/// The answer to the request on the wire, or never while none is out.
async fn answer(inflight: &mut Option<InFlight>) -> Outcome {
    match inflight {
        Some(inflight) => inflight.request.as_mut().await,
        None => std::future::pending().await,
    }
}

/// Sleep until `deadline`, or forever without one.
async fn at(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// The `failures`-th consecutive failure's wait: [`RETRY_BASE`] doubling up to [`RETRY_CAP`],
/// fully jittered: uniform in `[0, backoff]`.
fn backoff(failures: u32) -> Duration {
    let full = RETRY_BASE.saturating_mul(1 << failures.saturating_sub(1).min(16)).min(RETRY_CAP);
    full.mul_f64(rand::random::<f64>())
}

/// A cached batch is intact when it is one complete gzip member (RFC 1952) whose CRC checks
/// out: a truncated or bit-flipped file is caught here, never sent.
fn intact(body: &[u8]) -> bool {
    body.starts_with(&[0x1f, 0x8b]) && gunzip(body).is_some()
}

fn gunzip(body: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::with_capacity(body.len() * 4);
    flate2::read::GzDecoder::new(body).read_to_end(&mut out).ok()?;
    Some(out)
}

/// Level 1: protobuf with repeated attribute keys shrinks 5–10× already; higher levels buy little
/// for more CPU.
fn gzip(body: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::with_capacity(body.len() / 4), Compression::fast());
    // Writing into a Vec cannot fail.
    let _ = encoder.write_all(body);
    encoder.finish().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_ids_carry_their_owner() {
        let id =
            BatchId::format(Signal::Traces, "000007", 12, "abc", Some("my-proj.livekit.cloud"));
        let parsed = BatchId::parse(&id);
        assert_eq!((parsed.count, parsed.signal), (12, Signal::Traces));
        assert_eq!(parsed.owner, Some("abc"));
        assert_eq!(parsed.host, Some("my-proj.livekit.cloud"), "hosts may contain dashes");
        let process = BatchId::format(Signal::Logs, "000008", 3, "def", None);
        assert_eq!(BatchId::parse(&process).host, None);
        assert_eq!(events_in(&process), 3);
        assert!(id < process, "ids sort oldest first");
        let (a, b) = (BatchId::half(&id, 'a', 6), BatchId::half(&id, 'b', 6));
        assert!(id < a && a < b && b < process, "halves keep their place in line");
        assert_eq!((events_in(&a), BatchId::parse(&b).host), (6, parsed.host));
    }

    #[test]
    fn backoff_doubles_with_full_jitter_up_to_a_minute() {
        for (failures, full) in [(1, 1), (2, 2), (3, 4), (6, 32), (7, 60), (40, 60)] {
            let full = Duration::from_secs(full);
            let waits: Vec<Duration> = (0..64).map(|_| backoff(failures)).collect();
            assert!(waits.iter().all(|w| *w <= full), "{failures}: {waits:?} vs {full:?}");
            assert!(waits.iter().any(|w| *w < full / 2), "{failures}: jitter spans the range");
        }
    }
}
