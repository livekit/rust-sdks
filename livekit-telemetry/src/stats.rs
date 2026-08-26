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

use std::sync::atomic::{AtomicU64, Ordering};

use crate::TelemetryEvent;

/// Pipeline health counters, shared by the store, the exporter and [`Telemetry::stats`](crate::Telemetry::stats).
///
/// Reasons follow the OpenTelemetry SDK self-metrics conventions (`queue_full`, `rejected`,
/// `timeout`) so they line up with `otel.sdk.processor.log.processed` /
/// `otel.sdk.exporter.log.exported` on a backend that knows those.
#[derive(Default)]
pub(crate) struct Counters {
    /// Events evicted from the in-memory queue.
    pub queue_full: AtomicU64,
    /// Events lost because the cache could not store their batch (disk full, unusable dir).
    pub cache_error: AtomicU64,
    /// Events the collector rejected (4xx).
    pub rejected: AtomicU64,
    /// Events dropped inside a `Retry-After` window.
    pub throttled: AtomicU64,
    /// Events dropped after the collector disabled telemetry.
    pub disabled: AtomicU64,
    /// Batches the collector accepted.
    pub uploads_sent: AtomicU64,
    /// Upload attempts that failed transiently (network, timeout, 5xx).
    pub upload_failures: AtomicU64,
}

impl Counters {
    pub fn add(counter: &AtomicU64, n: u64) {
        counter.fetch_add(n, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Snapshot {
        let get = |c: &AtomicU64| c.load(Ordering::Relaxed);
        Snapshot {
            queue_full: get(&self.queue_full),
            cache_error: get(&self.cache_error),
            rejected: get(&self.rejected),
            throttled: get(&self.throttled),
            disabled: get(&self.disabled),
            uploads_sent: get(&self.uploads_sent),
            upload_failures: get(&self.upload_failures),
        }
    }
}

/// A point-in-time copy of [`Counters`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub queue_full: u64,
    pub cache_error: u64,
    pub rejected: u64,
    pub throttled: u64,
    pub disabled: u64,
    pub uploads_sent: u64,
    pub upload_failures: u64,
}

impl Snapshot {
    /// Counts accumulated since `earlier`.
    pub fn since(&self, earlier: &Snapshot) -> Snapshot {
        Snapshot {
            queue_full: self.queue_full.saturating_sub(earlier.queue_full),
            cache_error: self.cache_error.saturating_sub(earlier.cache_error),
            rejected: self.rejected.saturating_sub(earlier.rejected),
            throttled: self.throttled.saturating_sub(earlier.throttled),
            disabled: self.disabled.saturating_sub(earlier.disabled),
            uploads_sent: self.uploads_sent.saturating_sub(earlier.uploads_sent),
            upload_failures: self.upload_failures.saturating_sub(earlier.upload_failures),
        }
    }

    /// Anything worth telling the backend about: data lost or uploads failing.
    pub fn has_problems(&self) -> bool {
        self.queue_full + self.cache_error + self.rejected + self.throttled + self.upload_failures
            > 0
    }

    /// The `lk.telemetry.report` event: what this pipeline dropped or failed to upload since the
    /// previous report. The Sentry "client report" shape — deltas by reason, riding along with
    /// the next batch, never persisted on their own, never an extra request.
    pub fn report(&self, cached_batches: u64) -> TelemetryEvent {
        let mut event = TelemetryEvent::new("lk.telemetry.report")
            .with_attribute("lk.telemetry.uploads.failed", self.upload_failures as i64)
            .with_attribute("lk.telemetry.uploads.sent", self.uploads_sent as i64)
            .with_attribute("lk.telemetry.cache.batches", cached_batches as i64);
        for (key, value) in [
            ("lk.telemetry.dropped.queue_full", self.queue_full),
            ("lk.telemetry.dropped.cache_error", self.cache_error),
            ("lk.telemetry.dropped.rejected", self.rejected),
            ("lk.telemetry.dropped.throttled", self.throttled),
        ] {
            if value > 0 {
                event = event.with_attribute(key, value as i64);
            }
        }
        event
    }
}

/// Pipeline health as seen by the SDK: [`Telemetry::stats`](crate::Telemetry::stats).
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryStats {
    /// Events lost for any reason (sum of the `dropped_*` fields).
    pub dropped: u64,
    pub dropped_queue_full: u64,
    pub dropped_cache_error: u64,
    pub dropped_rejected: u64,
    pub dropped_throttled: u64,
    pub dropped_disabled: u64,
    /// Batches the collector accepted.
    pub uploads_sent: u64,
    /// Upload attempts that failed transiently.
    pub upload_failures: u64,
    /// Batches currently waiting in the cache.
    pub cached_batches: u64,
}

impl TelemetryStats {
    pub(crate) fn new(snapshot: Snapshot, cached_batches: u64) -> Self {
        Self {
            dropped: snapshot.queue_full
                + snapshot.cache_error
                + snapshot.rejected
                + snapshot.throttled
                + snapshot.disabled,
            dropped_queue_full: snapshot.queue_full,
            dropped_cache_error: snapshot.cache_error,
            dropped_rejected: snapshot.rejected,
            dropped_throttled: snapshot.throttled,
            dropped_disabled: snapshot.disabled,
            uploads_sent: snapshot.uploads_sent,
            upload_failures: snapshot.upload_failures,
            cached_batches,
        }
    }
}
