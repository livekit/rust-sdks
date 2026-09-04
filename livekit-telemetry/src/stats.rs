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
    /// Events evicted from the cache to stay under `max_cache_bytes` (or past the max age).
    pub cache_full: AtomicU64,
    /// Events the collector rejected (4xx).
    pub rejected: AtomicU64,
    /// Events dropped inside a `Retry-After` window.
    pub throttled: AtomicU64,
    /// Events dropped after the collector disabled telemetry.
    pub disabled: AtomicU64,
    /// Discrete events dropped by the flood guard (`max_events_per_10min`).
    pub rate_limited: AtomicU64,
    /// Batches the collector accepted.
    pub uploads_sent: AtomicU64,
    /// Compressed bytes the collector accepted — what telemetry actually cost the uplink.
    pub upload_bytes: AtomicU64,
    /// Upload attempts that failed transiently (network error, 5xx).
    pub upload_failures: AtomicU64,
    /// Upload attempts that hit `export_timeout_ms` (a slow network, or a stalled collector).
    pub upload_timeouts: AtomicU64,
    /// Upload holds that reached the 60 s cap and let one batch through: the policy was
    /// starving telemetry, and data arrived at least a minute late.
    pub hold_cap_hits: AtomicU64,
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
            cache_full: get(&self.cache_full),
            rejected: get(&self.rejected),
            throttled: get(&self.throttled),
            disabled: get(&self.disabled),
            rate_limited: get(&self.rate_limited),
            uploads_sent: get(&self.uploads_sent),
            upload_bytes: get(&self.upload_bytes),
            upload_failures: get(&self.upload_failures),
            upload_timeouts: get(&self.upload_timeouts),
            hold_cap_hits: get(&self.hold_cap_hits),
        }
    }
}

/// A point-in-time copy of [`Counters`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub queue_full: u64,
    pub cache_error: u64,
    pub cache_full: u64,
    pub rejected: u64,
    pub throttled: u64,
    pub disabled: u64,
    pub rate_limited: u64,
    pub uploads_sent: u64,
    pub upload_bytes: u64,
    pub upload_failures: u64,
    pub upload_timeouts: u64,
    pub hold_cap_hits: u64,
}

impl Snapshot {
    /// Counts accumulated since `earlier`.
    pub fn since(&self, earlier: &Snapshot) -> Snapshot {
        let d = |a: u64, b: u64| a.saturating_sub(b);
        Snapshot {
            queue_full: d(self.queue_full, earlier.queue_full),
            cache_error: d(self.cache_error, earlier.cache_error),
            cache_full: d(self.cache_full, earlier.cache_full),
            rejected: d(self.rejected, earlier.rejected),
            throttled: d(self.throttled, earlier.throttled),
            disabled: d(self.disabled, earlier.disabled),
            rate_limited: d(self.rate_limited, earlier.rate_limited),
            uploads_sent: d(self.uploads_sent, earlier.uploads_sent),
            upload_bytes: d(self.upload_bytes, earlier.upload_bytes),
            upload_failures: d(self.upload_failures, earlier.upload_failures),
            upload_timeouts: d(self.upload_timeouts, earlier.upload_timeouts),
            hold_cap_hits: d(self.hold_cap_hits, earlier.hold_cap_hits),
        }
    }

    /// Everything that was lost, by any reason.
    pub fn dropped(&self) -> u64 {
        self.queue_full
            + self.cache_error
            + self.cache_full
            + self.rejected
            + self.throttled
            + self.disabled
            + self.rate_limited
    }

    /// Anything worth telling the backend about: data lost, uploads failing, or the upload
    /// policy holding data back for a full minute.
    pub fn has_problems(&self) -> bool {
        self.dropped() - self.disabled
            + self.upload_failures
            + self.upload_timeouts
            + self.hold_cap_hits
            > 0
    }

    /// The `lk.telemetry.report` event: what this pipeline sent, dropped or failed to upload
    /// since the previous report. The Sentry "client report" shape — deltas by reason, riding
    /// along with the next batch, never persisted on their own, never an extra request — plus
    /// one at shutdown, so every session leaves a summary the fleet's success rates can be
    /// computed from.
    pub fn report(&self, cached_batches: u64) -> TelemetryEvent {
        let mut event = TelemetryEvent::new("lk.telemetry.report")
            .with_body(format!(
                "telemetry: {} batches sent ({} B), {} failed, {} dropped, {} cached",
                self.uploads_sent,
                self.upload_bytes,
                self.upload_failures + self.upload_timeouts,
                self.dropped() - self.disabled,
                cached_batches
            ))
            .with_attribute("lk.telemetry.uploads.sent", self.uploads_sent as i64)
            .with_attribute("lk.telemetry.uploads.bytes", self.upload_bytes as i64)
            .with_attribute("lk.telemetry.uploads.failed", self.upload_failures as i64)
            .with_attribute("lk.telemetry.cache.batches", cached_batches as i64);
        for (key, value) in [
            ("lk.telemetry.uploads.timeouts", self.upload_timeouts),
            ("lk.telemetry.holds.capped", self.hold_cap_hits),
            ("lk.telemetry.dropped.queue_full", self.queue_full),
            ("lk.telemetry.dropped.cache_error", self.cache_error),
            ("lk.telemetry.dropped.cache_full", self.cache_full),
            ("lk.telemetry.dropped.rejected", self.rejected),
            ("lk.telemetry.dropped.throttled", self.throttled),
            ("lk.telemetry.dropped.rate_limited", self.rate_limited),
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
    pub dropped_cache_full: u64,
    pub dropped_rejected: u64,
    pub dropped_throttled: u64,
    pub dropped_disabled: u64,
    pub dropped_rate_limited: u64,
    /// Batches the collector accepted.
    pub uploads_sent: u64,
    /// Compressed bytes the collector accepted.
    pub upload_bytes: u64,
    /// Upload attempts that failed transiently (network error, 5xx).
    pub upload_failures: u64,
    /// Upload attempts that timed out.
    pub upload_timeouts: u64,
    /// Upload holds that reached the 60 s cap.
    pub holds_capped: u64,
    /// Batches currently waiting in the cache.
    pub cached_batches: u64,
}

impl TelemetryStats {
    pub(crate) fn new(snapshot: Snapshot, cached_batches: u64) -> Self {
        Self {
            dropped: snapshot.dropped(),
            dropped_queue_full: snapshot.queue_full,
            dropped_cache_error: snapshot.cache_error,
            dropped_cache_full: snapshot.cache_full,
            dropped_rejected: snapshot.rejected,
            dropped_throttled: snapshot.throttled,
            dropped_disabled: snapshot.disabled,
            dropped_rate_limited: snapshot.rate_limited,
            uploads_sent: snapshot.uploads_sent,
            upload_bytes: snapshot.upload_bytes,
            upload_failures: snapshot.upload_failures,
            upload_timeouts: snapshot.upload_timeouts,
            holds_capped: snapshot.hold_cap_hits,
            cached_batches,
        }
    }
}
