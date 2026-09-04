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
    io::Write,
    sync::{Arc, Mutex},
    time::Duration,
};

use flate2::{write::GzEncoder, Compression};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, sleep_until, timeout, Instant};

use crate::{
    event::now_unix_nanos,
    otlp,
    rtc::StatsWindows,
    session::SessionState,
    span::Spans,
    stats::{Counters, Snapshot},
    store::{Queued, Store},
    telemetry::Destination,
    AppState, Attribute, BatchCache, DeviceState, ExportError, ExportRequest, TelemetryConfig,
    TelemetryTransport,
};

/// Retries per upload attempt after the first, for failures without `Retry-After`.
const MAX_RETRIES: u32 = 2;
const RETRY_BACKOFF: Duration = Duration::from_secs(1);
/// Pause before the cache is tried again after a failed upload (Sentry: stop consuming the
/// cache until `Retry-After` elapses; here also for plain connectivity failures).
const UPLOAD_BACKOFF: Duration = Duration::from_secs(60);
/// Uploads wait while the call is at its most sensitive (see [`Exporter::hold_reason`]) — but
/// never longer than this before one batch goes out anyway: the hard cap that bounds the policy
/// when its signals lie.
const MAX_HOLD: Duration = Duration::from_secs(60);
/// While one of these is open the uplink belongs to signaling and ICE/DTLS.
const SENSITIVE_SPANS: &[&str] = &["lk.connect", "lk.reconnect"];
/// RFC 9218 request priority: lowest urgency, not incremental. A hint for HTTP/2+ hops that
/// implement it; the host transport marks the local traffic class (see `TelemetryTransport`).
const PRIORITY: &str = "u=7";

/// Which OTLP signal a cached batch is; picks the endpoint at upload time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Signal {
    Logs,
    Traces,
}

impl Signal {
    fn tag(self) -> char {
        match self {
            Signal::Logs => 'l',
            Signal::Traces => 't',
        }
    }

    /// From a batch id's last component; ids from before signals existed are logs.
    fn of(id: &str) -> Signal {
        match id.rsplit('-').next() {
            Some("t") => Signal::Traces,
            _ => Signal::Logs,
        }
    }
}

pub(crate) enum Command {
    Flush(oneshot::Sender<()>),
    Shutdown(oneshot::Sender<()>),
    DeviceState(DeviceState),
    /// The queue crossed `flush_threshold_bytes`: export now rather than at the next tick.
    Overflow,
}

/// Outcome of uploading one encoded batch.
enum Delivery {
    Sent,
    /// Transient failure (network, timeout, 5xx) after retries — keep the batch.
    Failed,
    /// The collector asked us to back off; keep the batch, drop new ones meanwhile.
    Throttled {
        retry_after: Option<Duration>,
    },
    Rejected,
    Disabled,
}

/// Background actor that turns stored events into OTLP requests.
///
/// The role of OTel's `BatchLogRecordProcessor` + OTLP exporter in one place. Every tick it
/// [`enqueue`](Self::enqueue)s: drains up to `max_batch_size` events, appends an
/// `lk.telemetry.report` when something was dropped or failed since the last one, encodes and
/// gzips the batch and writes it to the [`BatchCache`] *before* any network is involved; then it
/// [`upload`](Self::upload)s the cache oldest-first through the [`TelemetryTransport`],
/// removing what the collector accepted or rejected. A failed upload pauses the cache for a
/// minute; a `Retry-After` additionally drops new batches for its duration (throttling must not
/// become a disk-backed queue); `Disabled` empties the cache and silences the exporter for good.
///
/// Telemetry must never win over media, so uploads are shaped as well as batched: at most
/// `max_batches_per_upload` per tick while a session may be live, and none at all while the room
/// is connecting or reconnecting or while the device asks for quiet ([`DeviceState::holds_uploads`])
/// — bounded by [`MAX_HOLD`]. Yielding to media on the wire is the transport's job: every request
/// carries `Priority: u=7` (RFC 9218), a gzipped body, and (on Apple) the background service class.
///
/// The tick period is `flush_interval_ms × cadence factor`: device pressure and a CPU-limited
/// encoder stretch it up to 4×, and entering the background flushes once immediately (the app
/// may be suspended any moment).
///
/// Drive it with `spawn(exporter.run())` on the consumer's runtime. It stops after
/// [`Telemetry::shutdown`](crate::Telemetry::shutdown) or when the last
/// [`Telemetry`](crate::Telemetry) handle is dropped.
pub struct Exporter {
    store: Arc<Store>,
    transport: Arc<dyn TelemetryTransport>,
    config: Arc<TelemetryConfig>,
    cache: Arc<dyn BatchCache>,
    counters: Arc<Counters>,
    windows: Arc<Mutex<StatsWindows>>,
    spans: Arc<Mutex<Spans>>,
    /// The pipeline's own session: self-telemetry is filed under it.
    process: Arc<SessionState>,
    /// Attributes attached to every record of every session (`Telemetry::set_attribute`).
    global: Arc<Mutex<Vec<Attribute>>>,
    /// Where batches go; `None` until `set_destination` — batches wait in the cache meanwhile.
    destination: Arc<Mutex<Option<Destination>>>,
    commands: mpsc::UnboundedReceiver<Command>,
    silenced: bool,
    /// Leave the cache alone until then: the last upload failed or we were throttled.
    paused_until: Option<Instant>,
    /// Drop new batches until then (`Retry-After` window).
    throttled_until: Option<Instant>,
    seq: u64,
    /// Batches the cache could not store (disk full, directory unusable). Drives one-shot logging.
    cache_failures: u64,
    /// Counter values at the last `lk.telemetry.report`.
    last_report: Snapshot,
    /// Shared with `Telemetry`: read synchronously, so a state pushed right before `emit` already
    /// governs the flush that follows.
    device: Arc<Mutex<Option<DeviceState>>>,
    /// When the current upload hold began, for the [`MAX_HOLD`] cap.
    held_since: Option<Instant>,
    /// Shutting down: the call is over, so only the device's own holds still apply.
    draining: bool,
    /// Append an `lk.telemetry.report` to the next batch even if nothing went wrong (the
    /// shutdown summary).
    force_report: bool,
}

impl Exporter {
    // Wiring, not an API: every handle is shared with `Telemetry`, which owns their creation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        store: Arc<Store>,
        transport: Arc<dyn TelemetryTransport>,
        config: Arc<TelemetryConfig>,
        cache: Arc<dyn BatchCache>,
        counters: Arc<Counters>,
        windows: Arc<Mutex<StatsWindows>>,
        spans: Arc<Mutex<Spans>>,
        process: Arc<SessionState>,
        global: Arc<Mutex<Vec<Attribute>>>,
        destination: Arc<Mutex<Option<Destination>>>,
        commands: mpsc::UnboundedReceiver<Command>,
        device: Arc<Mutex<Option<DeviceState>>>,
    ) -> Self {
        Self {
            store,
            transport,
            config,
            cache,
            counters,
            windows,
            spans,
            process,
            global,
            destination,
            commands,
            silenced: false,
            paused_until: None,
            throttled_until: None,
            seq: 0,
            cache_failures: 0,
            last_report: Snapshot::default(),
            device,
            held_since: None,
            draining: false,
            force_report: false,
        }
    }

    /// Run until shut down: replay whatever the cache holds, then export on every tick and on
    /// demand.
    pub async fn run(mut self) {
        self.upload().await;
        // Deadlines rather than tickers: `next = last + period`, re-derived every loop, so a
        // cadence change applies to the pending tick in both directions (pressure postpones it,
        // relief brings it forward) and a missed tick never bursts. The first flush is immediate,
        // the first window closes a full period after it opens.
        let mut last_flush = Instant::now() - self.period(self.config.flush_interval_ms);
        let mut last_window = Instant::now();
        loop {
            let next_flush = last_flush + self.period(self.config.flush_interval_ms);
            let next_window = last_window + self.period(self.config.stats_window_ms);
            tokio::select! {
                _ = sleep_until(next_flush) => {
                    self.export_pending().await;
                    last_flush = Instant::now();
                }
                _ = sleep_until(next_window) => {
                    self.close_windows();
                    last_window = Instant::now();
                }
                command = self.commands.recv() => match command {
                    Some(Command::Flush(done)) => {
                        self.export_pending().await;
                        let _ = done.send(());
                    }
                    Some(Command::Overflow) => self.export_pending().await,
                    Some(Command::DeviceState(state)) => {
                        if state.app_state == AppState::Background {
                            // The app may be suspended any moment: close the RTC windows and
                            // get everything into the cache (and out, if the network allows).
                            self.close_windows();
                            self.export_pending().await;
                        }
                    }
                    Some(Command::Shutdown(done)) => {
                        self.drain().await;
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
        self.device.lock().unwrap_or_else(|e| e.into_inner()).unwrap_or_default()
    }

    /// Device pressure, doubled again while WebRTC reports the encoder CPU-limited; capped at 4×.
    fn cadence_factor(&self) -> u32 {
        let cpu_limited = self.windows.lock().unwrap_or_else(|e| e.into_inner()).cpu_limited();
        (self.device().cadence_factor() * if cpu_limited { 2 } else { 1 }).min(4)
    }

    /// Last chance: everything queued into the cache and out. Ignores the upload backoff, the
    /// batch budget and the session holds (the call is over), respects throttling and the
    /// device's own holds.
    async fn drain(&mut self) {
        self.draining = true;
        self.force_report = true;
        self.paused_until = self.throttled_until;
        self.close_windows();
        self.export_pending().await;
        let left = self.cache.pending().len();
        if left > 0 {
            log::debug!("telemetry: {left} batches still cached at shutdown (replayed next start)");
        }
    }

    /// Turn every open RTC stats window into its `lk.rtc.stats.sample` event. Windows bypass the
    /// flood guard: they are the pipeline's own, bounded output.
    fn close_windows(&mut self) {
        let events = self.windows.lock().unwrap_or_else(|e| e.into_inner()).close();
        for event in events {
            self.store.push(event);
        }
    }

    async fn export_pending(&mut self) {
        self.enqueue();
        self.upload().await;
    }

    /// Encode everything queued — log records and finished spans — into the cache. No network.
    fn enqueue(&mut self) {
        self.enqueue_spans();
        loop {
            let mut batch = self.store.drain(
                self.config.max_batch_size.max(1) as usize,
                usize::try_from(self.config.max_batch_bytes.max(1)).unwrap_or(usize::MAX),
            );
            let throttled = self.throttled_until.is_some_and(|t| Instant::now() < t);
            // The shutdown summary goes out even with nothing else queued.
            let report_due = self.force_report && !self.silenced && !throttled;
            if batch.is_empty() && !report_due {
                return;
            }
            if self.silenced {
                Counters::add(&self.counters.disabled, batch.len() as u64);
                continue;
            }
            if throttled {
                Counters::add(&self.counters.throttled, batch.len() as u64);
                continue;
            }
            // Self-telemetry rides along with real data: never its own request, never its own
            // cadence, and only when there is something to report — plus once at shutdown.
            let now = self.counters.snapshot();
            let delta = now.since(&self.last_report);
            if delta.has_problems() || report_due {
                let cached = self.cache.pending().len() as u64;
                // The host's console gets the cumulative line through the FFI log path.
                log::debug!(
                    "{}",
                    crate::stats::TelemetryStats::new(self.counters.snapshot(), cached)
                );
                let report = delta.report(cached);
                batch.push(Queued { event: report, session: self.process.clone() });
                self.last_report = now;
                self.force_report = false;
            }
            let count = batch.len() as u64;
            let global = self.global.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let body = otlp::encode_logs(&self.config.resource, &global, batch);
            self.push_batch(Signal::Logs, count, &body);
        }
    }

    /// Finished spans travel as their own batch on the traces signal.
    fn enqueue_spans(&mut self) {
        let (spans, dropped) = {
            let mut registry = self.spans.lock().unwrap_or_else(|e| e.into_inner());
            (registry.drain(self.config.max_batch_size.max(1) as usize), registry.take_dropped())
        };
        Counters::add(&self.counters.queue_full, dropped);
        if spans.is_empty() {
            return;
        }
        if self.silenced || self.throttled_until.is_some_and(|t| Instant::now() < t) {
            let counter =
                if self.silenced { &self.counters.disabled } else { &self.counters.throttled };
            Counters::add(counter, spans.len() as u64);
            return;
        }
        let count = spans.len() as u64;
        let global = self.global.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let body = otlp::encode_spans(&self.config.resource, &global, spans);
        self.push_batch(Signal::Traces, count, &body);
    }

    /// Gzip the encoded batch and cache it. Compressed at rest as well as on the wire: the cache
    /// holds 5–10× more, the disk write shrinks, and a replay costs no CPU.
    fn push_batch(&mut self, signal: Signal, count: u64, body: &[u8]) {
        let body = gzip(body);
        self.seq += 1;
        let id = format!("{:020}-{:06}-{count}-{}", now_unix_nanos(), self.seq, signal.tag());
        match self.cache.push(&id, &body) {
            Ok(evicted) => {
                // Older batches pushed out by `max_cache_bytes`: lost, but counted.
                let lost: u64 = evicted.iter().map(|id| events_in(id)).sum();
                Counters::add(&self.counters.cache_full, lost);
            }
            Err(err) => {
                // A full disk is a steady state, not an event: warn once, then stay quiet.
                if self.cache_failures == 0 {
                    log::warn!(
                        "telemetry: cannot cache batches ({err}); dropping until it recovers"
                    );
                } else {
                    log::debug!("telemetry: could not cache {count} items: {err}");
                }
                self.cache_failures += 1;
                Counters::add(&self.counters.cache_error, count);
            }
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
        if self.spans.lock().unwrap_or_else(|e| e.into_inner()).any_open(SENSITIVE_SPANS) {
            return Some("connecting");
        }
        None
    }

    /// Send cached batches oldest-first, within this tick's budget, until one fails; then back
    /// off.
    async fn upload(&mut self) {
        if self.silenced {
            return;
        }
        if let Some(until) = self.paused_until.filter(|t| Instant::now() < *t) {
            log::debug!(
                "telemetry: upload paused for {}s more after a failure",
                (until - Instant::now()).as_secs()
            );
            return;
        }
        // No destination yet (SDK started, no room connected): everything waits in the cache.
        if self.destination.lock().unwrap_or_else(|e| e.into_inner()).is_none() {
            log::debug!(
                "telemetry: no destination yet; {} batches wait",
                self.cache.pending().len()
            );
            return;
        }
        let pending = self.cache.pending();
        if pending.is_empty() {
            self.held_since = None;
            return;
        }
        let budget = match self.hold_reason() {
            Some(reason) => {
                let since = *self.held_since.get_or_insert_with(Instant::now);
                if since.elapsed() < MAX_HOLD {
                    log::debug!("telemetry: holding {} batches: {reason}", pending.len());
                    return;
                }
                log::debug!(
                    "telemetry: hold capped after {}s ({reason}); sending one batch",
                    MAX_HOLD.as_secs()
                );
                // Held long enough: one batch goes out, then the hold starts over.
                self.held_since = Some(Instant::now());
                Counters::add(&self.counters.hold_cap_hits, 1);
                1
            }
            None => {
                self.held_since = None;
                if self.draining {
                    usize::MAX
                } else {
                    self.config.max_batches_per_upload.max(1) as usize
                }
            }
        };
        for id in pending.into_iter().take(budget) {
            let Some(body) = self.cache.read(&id) else {
                self.cache.remove(&id);
                continue;
            };
            match self.deliver(&body, Signal::of(&id)).await {
                Delivery::Sent => {
                    self.cache.remove(&id);
                    log::debug!(
                        "telemetry: sent {} B, {} batches left",
                        body.len(),
                        self.cache.pending().len()
                    );
                    Counters::add(&self.counters.uploads_sent, 1);
                    Counters::add(&self.counters.upload_bytes, body.len() as u64);
                }
                Delivery::Rejected => {
                    self.cache.remove(&id);
                    Counters::add(&self.counters.rejected, events_in(&id));
                }
                Delivery::Failed => {
                    self.paused_until = Some(Instant::now() + UPLOAD_BACKOFF);
                    return;
                }
                Delivery::Throttled { retry_after } => {
                    let until = Instant::now() + retry_after.unwrap_or(UPLOAD_BACKOFF);
                    self.paused_until = Some(until);
                    self.throttled_until = Some(until);
                    return;
                }
                Delivery::Disabled => {
                    self.silence();
                    return;
                }
            }
        }
    }

    /// Telemetry is disabled for this project: never send again, and never replay what is cached.
    fn silence(&mut self) {
        log::warn!("telemetry disabled by the collector; going silent");
        self.silenced = true;
        let cached: u64 = self.cache.pending().iter().map(|id| events_in(id)).sum();
        Counters::add(&self.counters.disabled, cached);
        self.cache.clear();
    }

    /// Upload one cached (gzipped) batch with bounded retries and classify the outcome.
    async fn deliver(&self, body: &[u8], signal: Signal) -> Delivery {
        let Some(destination) = self.destination.lock().unwrap_or_else(|e| e.into_inner()).clone()
        else {
            return Delivery::Failed;
        };
        let mut headers = destination.headers;
        headers.insert("Content-Type".to_owned(), otlp::CONTENT_TYPE.to_owned());
        headers.insert("Content-Encoding".to_owned(), "gzip".to_owned());
        headers.insert("Priority".to_owned(), PRIORITY.to_owned());
        let url = match signal {
            Signal::Logs => destination.logs,
            Signal::Traces => destination.traces,
        };
        let request = ExportRequest { url, headers, body: body.to_vec() };
        let attempt_timeout = Duration::from_millis(self.config.export_timeout_ms.max(1));

        // ponytail: linear backoff, 2 retries, blocks the tick loop while sleeping (≤ 3 s).
        // Exponential + jitter once real fleets exercise this.
        for attempt in 0..=MAX_RETRIES {
            match timeout(attempt_timeout, self.transport.send(request.clone())).await {
                Ok(Ok(())) => return Delivery::Sent,
                Ok(Err(ExportError::Disabled)) => return Delivery::Disabled,
                Ok(Err(ExportError::Rejected { message })) => {
                    log::warn!("telemetry batch rejected: {message}");
                    return Delivery::Rejected;
                }
                Ok(Err(ExportError::Retryable { message, retry_after_ms: Some(ms) })) => {
                    log::debug!("telemetry throttled for {ms} ms: {message}");
                    return Delivery::Throttled { retry_after: Some(Duration::from_millis(ms)) };
                }
                Ok(Err(ExportError::Retryable { message, retry_after_ms: None })) => {
                    log::debug!("telemetry upload failed (attempt {}): {message}", attempt + 1);
                    Counters::add(&self.counters.upload_failures, 1);
                }
                Err(_) => {
                    log::debug!("telemetry upload timed out (attempt {})", attempt + 1);
                    Counters::add(&self.counters.upload_timeouts, 1);
                }
            }
            if attempt < MAX_RETRIES {
                sleep(RETRY_BACKOFF * (attempt + 1)).await;
            }
        }
        Delivery::Failed
    }
}

/// Level 1: protobuf with repeated attribute keys shrinks 5–10× already; higher levels buy little
/// for more CPU.
fn gzip(body: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::with_capacity(body.len() / 4), Compression::fast());
    // Writing into a Vec cannot fail.
    let _ = encoder.write_all(body);
    encoder.finish().unwrap_or_default()
}

/// The event count the exporter encodes as the last component of a batch id.
fn events_in(id: &str) -> u64 {
    id.split('-').nth(2).and_then(|n| n.parse().ok()).unwrap_or(0)
}
