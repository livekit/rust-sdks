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
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::time::{interval, sleep, timeout, Instant, MissedTickBehavior};
use tokio::{
    sync::{mpsc, oneshot},
    time::Interval,
};

use crate::{
    event::now_unix_nanos,
    otlp,
    rtc::StatsWindows,
    span::Spans,
    stats::{Counters, Snapshot},
    store::Store,
    AppState, Attribute, BatchCache, DeviceState, ExportError, ExportRequest, TelemetryConfig,
    TelemetryTransport,
};

/// Retries per upload attempt after the first, for failures without `Retry-After`.
const MAX_RETRIES: u32 = 2;
const RETRY_BACKOFF: Duration = Duration::from_secs(1);
/// Pause before the cache is tried again after a failed upload (Sentry: stop consuming the
/// cache until `Retry-After` elapses; here also for plain connectivity failures).
const UPLOAD_BACKOFF: Duration = Duration::from_secs(60);

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
/// `lk.telemetry.report` when something was dropped or failed since the last one, encodes the
/// batch and writes it to the [`BatchCache`] *before* any network is involved; then it
/// [`upload`](Self::upload)s the cache oldest-first through the [`TelemetryTransport`],
/// removing what the collector accepted or rejected. A failed upload pauses the cache for a
/// minute; a `Retry-After` additionally drops new batches for its duration (throttling must not
/// become a disk-backed queue); `Disabled` empties the cache and silences the exporter for good.
///
/// The tick period is `flush_interval_ms × DeviceState::cadence_factor`: thermal pressure,
/// low-power mode and the background stretch it up to 4×, and entering the background flushes
/// once immediately (the app may be suspended any moment).
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
    /// Session-wide attributes merged into every record at encode time.
    attributes: Arc<Mutex<Vec<Attribute>>>,
    spans: Arc<Mutex<Spans>>,
    trace_id: [u8; 16],
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
    cadence_factor: u32,
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
        attributes: Arc<Mutex<Vec<Attribute>>>,
        spans: Arc<Mutex<Spans>>,
        trace_id: [u8; 16],
        commands: mpsc::UnboundedReceiver<Command>,
    ) -> Self {
        Self {
            store,
            transport,
            config,
            cache,
            counters,
            windows,
            attributes,
            spans,
            trace_id,
            commands,
            silenced: false,
            paused_until: None,
            throttled_until: None,
            seq: 0,
            cache_failures: 0,
            last_report: Snapshot::default(),
            cadence_factor: 1,
        }
    }

    /// Run until shut down: replay whatever the cache holds, then export on every tick and on
    /// demand.
    pub async fn run(mut self) {
        self.upload().await;
        let mut ticker = self.ticker(self.config.flush_interval_ms);
        let mut stats_ticker = self.window_ticker();
        loop {
            tokio::select! {
                _ = ticker.tick() => self.export_pending().await,
                _ = stats_ticker.tick() => self.close_windows(),
                command = self.commands.recv() => match command {
                    Some(Command::Flush(done)) => {
                        self.export_pending().await;
                        let _ = done.send(());
                    }
                    Some(Command::DeviceState(state)) => {
                        let factor = state.cadence_factor();
                        if factor != self.cadence_factor {
                            self.cadence_factor = factor;
                            ticker = self.ticker(self.config.flush_interval_ms);
                            stats_ticker = self.window_ticker();
                        }
                        if state.app_state == AppState::Background {
                            // The app may be suspended any moment: close the RTC windows and
                            // get everything into the cache (and out, if the network allows).
                            self.close_windows();
                            self.export_pending().await;
                        }
                    }
                    Some(Command::Shutdown(done)) => {
                        // Last chance: ignore the upload backoff, but respect throttling.
                        self.paused_until = self.throttled_until;
                        self.close_windows();
                        self.export_pending().await;
                        let _ = done.send(());
                        return;
                    }
                    // Every `Telemetry` handle is gone.
                    None => {
                        self.paused_until = self.throttled_until;
                        self.close_windows();
                        self.export_pending().await;
                        return;
                    }
                },
            }
        }
    }

    /// A ticker at `base_ms × cadence_factor`.
    fn ticker(&self, base_ms: u64) -> Interval {
        let period = Duration::from_millis(base_ms.max(1)) * self.cadence_factor;
        let mut ticker = interval(period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        ticker
    }

    /// The stats-window ticker. Unlike `interval`, whose first tick is immediate, the first window
    /// closes a full period after it opens — otherwise readings taken before the first tick
    /// would ship as a zero-length window.
    fn window_ticker(&self) -> Interval {
        let period =
            Duration::from_millis(self.config.stats_window_ms.max(1)) * self.cadence_factor;
        let mut ticker = tokio::time::interval_at(Instant::now() + period, period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        ticker
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
            let mut batch = self.store.drain(self.config.max_batch_size.max(1) as usize);
            if batch.is_empty() {
                return;
            }
            if self.silenced {
                Counters::add(&self.counters.disabled, batch.len() as u64);
                continue;
            }
            if self.throttled_until.is_some_and(|t| Instant::now() < t) {
                Counters::add(&self.counters.throttled, batch.len() as u64);
                continue;
            }
            // Self-telemetry rides along with real data: never its own request, never its own
            // cadence, and only when there is something to report.
            let now = self.counters.snapshot();
            let delta = now.since(&self.last_report);
            if delta.has_problems() {
                batch.push(delta.report(self.cache.pending().len() as u64));
                self.last_report = now;
            }
            for event in &mut batch {
                self.attach_session_attributes(&mut event.attributes);
            }
            let count = batch.len() as u64;
            let body = otlp::encode_logs(&self.config.resource, &self.trace_id, batch);
            self.push_batch(Signal::Logs, count, &body);
        }
    }

    /// Finished spans travel as their own batch on the traces signal.
    fn enqueue_spans(&mut self) {
        let (mut spans, dropped) = {
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
        for span in &mut spans {
            self.attach_session_attributes(&mut span.attributes);
        }
        let count = spans.len() as u64;
        let body = otlp::encode_spans(&self.config.resource, &self.trace_id, spans);
        self.push_batch(Signal::Traces, count, &body);
    }

    fn push_batch(&mut self, signal: Signal, count: u64, body: &[u8]) {
        self.seq += 1;
        let id = format!("{:020}-{:06}-{count}-{}", now_unix_nanos(), self.seq, signal.tag());
        if let Err(err) = self.cache.push(&id, body) {
            // A full disk is a steady state, not an event: warn once, then stay quiet.
            if self.cache_failures == 0 {
                log::warn!("telemetry: cannot cache batches ({err}); dropping until it recovers");
            } else {
                log::debug!("telemetry: could not cache {count} items: {err}");
            }
            self.cache_failures += 1;
            Counters::add(&self.counters.cache_error, count);
        }
    }

    /// Merge the session-wide attributes into a record's own, without overriding explicit ones.
    fn attach_session_attributes(&self, attributes: &mut Vec<Attribute>) {
        let session = self.attributes.lock().unwrap_or_else(|e| e.into_inner());
        for attribute in session.iter() {
            if !attributes.iter().any(|a| a.key == attribute.key) {
                attributes.push(attribute.clone());
            }
        }
    }

    /// Send cached batches oldest-first until one fails; then back off.
    async fn upload(&mut self) {
        if self.silenced || self.paused_until.is_some_and(|t| Instant::now() < t) {
            return;
        }
        for id in self.cache.pending() {
            let Some(body) = self.cache.read(&id) else {
                self.cache.remove(&id);
                continue;
            };
            match self.deliver(&body, Signal::of(&id)).await {
                Delivery::Sent => {
                    self.cache.remove(&id);
                    Counters::add(&self.counters.uploads_sent, 1);
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

    /// Upload one encoded batch with bounded retries and classify the outcome.
    async fn deliver(&self, body: &[u8], signal: Signal) -> Delivery {
        let mut headers = self.config.headers.clone();
        headers.insert("Content-Type".to_owned(), otlp::CONTENT_TYPE.to_owned());
        let url = match signal {
            Signal::Logs => self.config.endpoint.clone(),
            Signal::Traces => {
                self.config.traces_endpoint.clone().unwrap_or_else(|| self.config.endpoint.clone())
            }
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
                    Counters::add(&self.counters.upload_failures, 1);
                }
            }
            if attempt < MAX_RETRIES {
                sleep(RETRY_BACKOFF * (attempt + 1)).await;
            }
        }
        Delivery::Failed
    }
}

/// The event count the exporter encodes as the last component of a batch id.
fn events_in(id: &str) -> u64 {
    id.split('-').nth(2).and_then(|n| n.parse().ok()).unwrap_or(0)
}
