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

use std::{sync::Arc, time::Duration};

use livekit_runtime::{interval, sleep, timeout, Instant, MissedTickBehavior};
use tokio::sync::{mpsc, oneshot};

use crate::{
    otlp, persist::FileCache, store::Store, ExportError, ExportRequest, TelemetryConfig,
    TelemetryEvent, TelemetryTransport,
};

/// Retries per batch after the first attempt. Fixed for now; see [`Exporter::deliver`].
const MAX_RETRIES: u32 = 2;
const RETRY_BACKOFF: Duration = Duration::from_secs(1);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);
/// Pause before the on-disk cache is retried after a failed replay (Sentry: stop consuming the
/// cache until `Retry-After` elapses; here also for plain connectivity failures).
const REPLAY_BACKOFF: Duration = Duration::from_secs(60);

pub(crate) enum Command {
    Flush(oneshot::Sender<()>),
    Shutdown(oneshot::Sender<()>),
}

/// Outcome of delivering one encoded batch, after retries.
enum Delivery {
    Sent,
    /// Transient failure (network, timeout, 5xx) — worth keeping for later.
    Failed,
    /// The collector asked us to back off (`Retry-After`) — dropped, never persisted.
    Throttled,
    Rejected,
    Disabled,
}

/// Background actor that turns stored events into OTLP requests.
///
/// The role of OTel's `BatchLogRecordProcessor` + OTLP exporter in one place: every
/// `flush_interval_ms` it drains up to `max_batch_size` events, encodes them, and hands the
/// request to the [`TelemetryTransport`]. Transient failures are retried with a bounded
/// backoff (honoring `Retry-After`), rejected batches are dropped, and once the collector
/// reports telemetry as disabled the exporter goes silent for good — dropping instead of
/// retrying, so a disabled project never sees a request storm.
///
/// With `storage_dir` configured, batches that still fail after retries are written to a
/// [`FileCache`] and replayed oldest-first on the next tick (after a backoff) and on the next
/// launch; `shutdown` spills whatever is queued to disk *before* trying the network, so an app
/// being killed offline loses nothing. Throttled, rejected and disabled data is never persisted.
///
/// Drive it with `spawn(exporter.run())` on the consumer's runtime. It stops after
/// [`Telemetry::shutdown`](crate::Telemetry::shutdown) or when the last
/// [`Telemetry`](crate::Telemetry) handle is dropped.
pub struct Exporter {
    store: Arc<Store>,
    transport: Arc<dyn TelemetryTransport>,
    config: Arc<TelemetryConfig>,
    cache: Option<Arc<FileCache>>,
    commands: mpsc::UnboundedReceiver<Command>,
    silenced: bool,
    replay_after: Option<Instant>,
    /// Batches the cache could not store (disk full, directory unusable). Drives one-shot logging.
    persist_failures: u64,
}

impl Exporter {
    pub(crate) fn new(
        store: Arc<Store>,
        transport: Arc<dyn TelemetryTransport>,
        config: Arc<TelemetryConfig>,
        cache: Option<Arc<FileCache>>,
        commands: mpsc::UnboundedReceiver<Command>,
    ) -> Self {
        Self {
            store,
            transport,
            config,
            cache,
            commands,
            silenced: false,
            replay_after: None,
            persist_failures: 0,
        }
    }

    /// Run until shut down. Replays persisted batches, then exports on every tick and on demand.
    pub async fn run(mut self) {
        self.replay().await;
        let mut ticker = interval(Duration::from_millis(self.config.flush_interval_ms.max(1)));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    self.replay().await;
                    self.export_pending(false).await;
                }
                command = self.commands.recv() => match command {
                    Some(Command::Flush(done)) => {
                        self.export_pending(true).await;
                        let _ = done.send(());
                    }
                    Some(Command::Shutdown(done)) => {
                        self.shutdown().await;
                        let _ = done.send(());
                        return;
                    }
                    // Every `Telemetry` handle is gone.
                    None => {
                        self.shutdown().await;
                        return;
                    }
                },
            }
        }
    }

    /// Final flush. With a cache: spill everything to disk first (cheap, local), then send as
    /// much as the network allows — whatever remains is replayed on the next launch.
    async fn shutdown(&mut self) {
        if self.cache.is_some() && !self.silenced {
            self.spill();
            self.replay_after = None;
            self.replay().await;
        } else {
            self.export_pending(true).await;
        }
    }

    /// Export one batch, or everything queued when `drain_all` is set.
    async fn export_pending(&mut self, drain_all: bool) {
        loop {
            let batch = self.store.drain(self.config.max_batch_size.max(1) as usize);
            if batch.is_empty() {
                return;
            }
            self.export(batch).await;
            if !drain_all {
                return;
            }
        }
    }

    async fn export(&mut self, batch: Vec<TelemetryEvent>) {
        let count = batch.len() as u64;
        if self.silenced {
            self.store.add_dropped(count);
            return;
        }
        let body = otlp::encode_logs(&self.config.resource, batch);
        match self.deliver(&body).await {
            Delivery::Sent => {}
            Delivery::Failed => {
                self.persist_or_drop(&body, count);
                // The network just failed; do not hammer it with the cache on the next tick.
                self.replay_after = Some(Instant::now() + REPLAY_BACKOFF);
            }
            Delivery::Throttled | Delivery::Rejected => self.store.add_dropped(count),
            Delivery::Disabled => {
                self.silence();
                self.store.add_dropped(count);
            }
        }
    }

    /// Encode everything queued straight to disk, without touching the network.
    fn spill(&mut self) {
        loop {
            let batch = self.store.drain(self.config.max_batch_size.max(1) as usize);
            if batch.is_empty() {
                return;
            }
            let count = batch.len() as u64;
            let body = otlp::encode_logs(&self.config.resource, batch);
            self.persist_or_drop(&body, count);
        }
    }

    /// Send persisted batches oldest-first until one fails; then back off.
    async fn replay(&mut self) {
        let Some(cache) = self.cache.clone() else { return };
        if self.silenced || self.replay_after.is_some_and(|t| Instant::now() < t) {
            return;
        }
        for path in cache.pending() {
            let Ok(body) = cache.read(&path) else {
                cache.remove(&path);
                continue;
            };
            match self.deliver(&body).await {
                Delivery::Sent | Delivery::Rejected => cache.remove(&path),
                Delivery::Failed | Delivery::Throttled => {
                    self.replay_after = Some(Instant::now() + REPLAY_BACKOFF);
                    return;
                }
                Delivery::Disabled => {
                    self.silence();
                    return;
                }
            }
        }
    }

    fn persist_or_drop(&mut self, body: &[u8], count: u64) {
        match self.cache.as_deref().map(|cache| cache.store(body)) {
            Some(Ok(())) => log::debug!("telemetry: persisted {count} events for later delivery"),
            Some(Err(err)) => {
                // Disk full is a steady state, not an event: warn once, then stay quiet.
                if self.persist_failures == 0 {
                    log::warn!(
                        "telemetry: cannot persist events ({err}); dropping until it recovers"
                    );
                } else {
                    log::debug!("telemetry: could not persist {count} events: {err}");
                }
                self.persist_failures += 1;
                self.store.add_dropped(count);
            }
            None => {
                log::warn!("telemetry: export failed, dropping {count} events");
                self.store.add_dropped(count);
            }
        }
    }

    /// Telemetry is disabled for this project: never send again, and never replay what is on disk.
    fn silence(&mut self) {
        log::warn!("telemetry disabled by the collector; going silent");
        self.silenced = true;
        if let Some(cache) = &self.cache {
            cache.clear();
        }
    }

    /// Deliver one encoded batch with bounded retries and classify the outcome.
    async fn deliver(&self, body: &[u8]) -> Delivery {
        let mut headers = self.config.headers.clone();
        headers.insert("Content-Type".to_owned(), otlp::CONTENT_TYPE.to_owned());
        let request =
            ExportRequest { url: self.config.endpoint.clone(), headers, body: body.to_vec() };
        let attempt_timeout = Duration::from_millis(self.config.export_timeout_ms.max(1));

        // ponytail: linear backoff, 2 retries, blocks the tick loop while sleeping (bounded by
        // MAX_RETRIES × MAX_RETRY_DELAY). Exponential + jitter once real fleets exercise this.
        let mut throttled = false;
        for attempt in 0..=MAX_RETRIES {
            let delay = match timeout(attempt_timeout, self.transport.send(request.clone())).await {
                Ok(Ok(())) => return Delivery::Sent,
                Ok(Err(ExportError::Disabled)) => return Delivery::Disabled,
                Ok(Err(ExportError::Rejected { message })) => {
                    log::warn!("telemetry batch rejected: {message}");
                    return Delivery::Rejected;
                }
                Ok(Err(ExportError::Retryable { message, retry_after_ms })) => {
                    log::debug!("telemetry export failed (attempt {}): {message}", attempt + 1);
                    throttled = retry_after_ms.is_some();
                    retry_after_ms
                        .map(Duration::from_millis)
                        .unwrap_or(RETRY_BACKOFF * (attempt + 1))
                }
                Err(_) => {
                    log::debug!("telemetry export timed out (attempt {})", attempt + 1);
                    throttled = false;
                    RETRY_BACKOFF * (attempt + 1)
                }
            };
            if attempt < MAX_RETRIES {
                sleep(delay.min(MAX_RETRY_DELAY)).await;
            }
        }
        if throttled {
            Delivery::Throttled
        } else {
            Delivery::Failed
        }
    }
}
