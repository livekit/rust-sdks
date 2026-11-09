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

use livekit_protocol::semconv::{self, event};

use crate::TelemetryEvent;

/// Declares the pipeline's health counters once: the shared atomics ([`Counters`]), their
/// point-in-time copy ([`Snapshot`]) and the delta between two copies.
macro_rules! counters {
    ($($(#[$doc:meta])* $name:ident,)*) => {
        /// Pipeline health counters, shared by the store, the exporter and
        /// [`Telemetry::stats`](crate::Telemetry::stats). Loss reasons follow the OpenTelemetry
        /// SDK self-metrics conventions (`queue_full`, `rejected`, `timeout`) where one exists.
        #[derive(Default)]
        pub(crate) struct Counters {
            $($(#[$doc])* pub $name: AtomicU64,)*
        }

        /// A point-in-time copy of [`Counters`].
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub(crate) struct Snapshot {
            $(pub $name: u64,)*
        }

        impl Counters {
            pub fn snapshot(&self) -> Snapshot {
                Snapshot { $($name: self.$name.load(Ordering::Relaxed),)* }
            }
        }

        impl Snapshot {
            /// Counts accumulated since `earlier`.
            pub fn since(&self, earlier: &Snapshot) -> Snapshot {
                Snapshot { $($name: self.$name.saturating_sub(earlier.$name),)* }
            }
        }
    };
}

counters! {
    /// Records evicted from the in-memory queue (`max_queue_size`), spans dropped from a full
    /// span buffer (open spans abandoned, finished ones past its capacity), and tracks a session
    /// did not wait on past `MAX_PENDING_TRACKS` (a subscribe's span, a published track's fast
    /// polling).
    queue_full,
    /// Records lost because no cache, not even memory, could take their batch.
    cache_error,
    /// Records evicted from the cache by its size or file-count bound.
    cache_full,
    /// Records in cached batches past the 24 h age limit.
    expired,
    /// Records in cached batches that failed their integrity check (truncated, corrupt).
    corrupt,
    /// Records the core refused at the door: a custom event or attribute over the limits.
    invalid,
    /// Records the collector rejected: a final 4xx/5xx, or refused in a partial success.
    rejected,
    /// Single records larger than the collector accepts (413 down to one record).
    oversized,
    /// Records evicted from the cache while the collector held uploads off (`Retry-After`).
    throttled,
    /// Records for a project that receives nothing (self-hosted, no grant, disabled, 404).
    disabled,
    /// Discrete events dropped by the flood guard (`max_events_per_10min`).
    rate_limited,
    /// Records deleted by the opt-out.
    purged,
    /// Batches the collector accepted.
    uploads_sent,
    /// Compressed bytes the collector accepted — what telemetry actually cost the uplink.
    upload_bytes,
    /// Upload attempts that failed transiently before the timeout (a network error, 429, 5xx).
    upload_failures,
    /// Upload attempts that hit `export_timeout_ms` (a slow network, or a stalled collector).
    upload_timeouts,
    /// 401/403 answers: a token the collector refused (the batches wait for the next one).
    auth_denied,
    /// Soft holds that reached the cap and let one batch through: the policy was starving
    /// telemetry, and data arrived late.
    hold_cap_hits,
    /// Batches the disk cache could not store (disk full, directory gone), kept in memory
    /// instead: they survive a failed upload, not the process.
    cache_write_errors,
}

impl Counters {
    pub fn add(counter: &AtomicU64, n: u64) {
        counter.fetch_add(n, Ordering::Relaxed);
    }
}

impl Snapshot {
    /// Everything that was lost, by any reason.
    pub fn dropped(&self) -> u64 {
        self.problem_drops() + self.disabled + self.purged
    }

    /// Upload attempts that did not get through: failures and timeouts together, the "failed"
    /// of the report's body (the `lk.telemetry.uploads.failed` attribute counts failures only).
    pub fn failed_attempts(&self) -> u64 {
        self.upload_failures + self.upload_timeouts
    }

    /// Records lost other than by policy (a project that receives nothing, an opt-out).
    pub fn problem_drops(&self) -> u64 {
        self.queue_full
            + self.cache_error
            + self.cache_full
            + self.expired
            + self.corrupt
            + self.invalid
            + self.rejected
            + self.oversized
            + self.throttled
            + self.rate_limited
    }

    /// Anything worth telling the backend about: data lost (other than by policy), uploads
    /// failing or refused, holds starving uploads, the disk refusing writes.
    pub fn has_problems(&self) -> bool {
        self.problem_drops() > 0
            || self.upload_failures > 0
            || self.upload_timeouts > 0
            || self.auth_denied > 0
            || self.hold_cap_hits > 0
            || self.cache_write_errors > 0
    }

    /// The `lk.telemetry.report` event: what this pipeline sent, dropped or failed to upload
    /// since the previous report — deltas by reason, riding along with the next batch, never
    /// persisted on its own, never an extra request — plus one at shutdown, so every session
    /// leaves a summary the fleet's success rates can be computed from.
    pub fn report(&self, cached_batches: u64) -> TelemetryEvent {
        let mut event = TelemetryEvent::new(event::LK_TELEMETRY_REPORT)
            .with_body(format!(
                "telemetry: {} batches sent ({} B), {} failed, {} dropped, {} cached",
                self.uploads_sent,
                self.upload_bytes,
                self.failed_attempts(),
                self.problem_drops(),
                cached_batches
            ))
            .with_attribute(semconv::LK_TELEMETRY_UPLOADS_SENT, int(self.uploads_sent))
            .with_attribute(semconv::LK_TELEMETRY_UPLOADS_BYTES, int(self.upload_bytes))
            .with_attribute(semconv::LK_TELEMETRY_UPLOADS_FAILED, int(self.upload_failures))
            .with_attribute(semconv::LK_TELEMETRY_CACHE_BATCHES, int(cached_batches));
        for (key, value) in [
            (semconv::LK_TELEMETRY_UPLOADS_TIMEOUTS, self.upload_timeouts),
            (semconv::LK_TELEMETRY_UPLOADS_UNAUTHORIZED, self.auth_denied),
            (semconv::LK_TELEMETRY_HOLDS_CAPPED, self.hold_cap_hits),
            (semconv::LK_TELEMETRY_CACHE_WRITE_ERRORS, self.cache_write_errors),
            (semconv::LK_TELEMETRY_DROPPED_QUEUE_FULL, self.queue_full),
            (semconv::LK_TELEMETRY_DROPPED_CACHE_ERROR, self.cache_error),
            (semconv::LK_TELEMETRY_DROPPED_CACHE_FULL, self.cache_full),
            (semconv::LK_TELEMETRY_DROPPED_EXPIRED, self.expired),
            (semconv::LK_TELEMETRY_DROPPED_CORRUPT, self.corrupt),
            (semconv::LK_TELEMETRY_DROPPED_INVALID, self.invalid),
            (semconv::LK_TELEMETRY_DROPPED_REJECTED, self.rejected),
            (semconv::LK_TELEMETRY_DROPPED_OVERSIZED, self.oversized),
            (semconv::LK_TELEMETRY_DROPPED_THROTTLED, self.throttled),
            (semconv::LK_TELEMETRY_DROPPED_RATE_LIMITED, self.rate_limited),
        ] {
            if value > 0 {
                event = event.with_attribute(key, int(value));
            }
        }
        event
    }
}

/// A count as an OTLP integer: past `i64::MAX` it saturates rather than wraps negative.
fn int(count: u64) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

/// What the upload policy is doing right now.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryStatus {
    /// Uploading as data arrives.
    Ok,
    /// Uploads wait for a connect/reconnect to finish or for the device (Low Data Mode, low
    /// battery); capped at 60 s.
    Held,
    /// The last upload failed; retrying after a jittered exponential backoff.
    Paused,
    /// The collector asked for a pause (`Retry-After`).
    Throttled,
    /// No destination or no usable token yet (no room connected, the token expired, lacks the
    /// grant or was refused); everything waits in the cache for the next token.
    Waiting,
    /// No project receives telemetry: every server this process talked to is self-hosted, never
    /// granted observability, has no ingest, or disabled data recording.
    Off,
}

impl std::fmt::Display for TelemetryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Ok => "ok",
            Self::Held => "held",
            Self::Paused => "paused",
            Self::Throttled => "throttled",
            Self::Waiting => "waiting",
            Self::Off => "off",
        })
    }
}

/// Pipeline health as seen by the SDK: [`Telemetry::stats`](crate::Telemetry::stats).
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryStats {
    /// What the upload policy is doing right now.
    pub status: TelemetryStatus,
    /// Records lost for any reason (sum of the `dropped_*` fields).
    pub dropped: u64,
    /// Records evicted from the in-memory queue, spans dropped from a full span buffer, and
    /// tracks a session did not wait on past 256 at once (a subscribe loses its span, a published
    /// track its fast polling).
    pub dropped_queue_full: u64,
    /// Records lost because no cache, not even memory, could take their batch.
    pub dropped_cache_error: u64,
    /// Records evicted from the cache by its size or file-count bound.
    pub dropped_cache_full: u64,
    /// Records in cached batches past the 24 h age limit.
    pub dropped_expired: u64,
    /// Records in cached batches that failed their integrity check.
    pub dropped_corrupt: u64,
    /// Custom events and attributes refused at the door, over the limits.
    pub dropped_invalid: u64,
    /// Records the collector rejected (a final 4xx/5xx, or a partial success).
    pub dropped_rejected: u64,
    /// Single records larger than the collector accepts.
    pub dropped_oversized: u64,
    /// Records evicted from the cache while the collector held uploads off.
    pub dropped_throttled: u64,
    /// Records for a project that receives nothing (self-hosted, no grant, disabled, 404).
    pub dropped_disabled: u64,
    /// Discrete events dropped by the flood guard.
    pub dropped_rate_limited: u64,
    /// Records deleted by the opt-out.
    pub dropped_purged: u64,
    /// Batches the collector accepted.
    pub uploads_sent: u64,
    /// Compressed bytes the collector accepted.
    pub upload_bytes: u64,
    /// Upload attempts that failed transiently before the timeout (a network error, 429, 5xx).
    pub upload_failures: u64,
    /// Upload attempts that got no answer within `export_timeout_ms`; not counted as failures.
    pub upload_timeouts: u64,
    /// Tokens the collector refused (401/403).
    pub uploads_unauthorized: u64,
    /// Soft holds that reached the cap.
    pub holds_capped: u64,
    /// Batches the disk cache could not store, kept in memory instead.
    pub cache_write_errors: u64,
    /// Batches currently waiting in the cache.
    pub cached_batches: u64,
}

/// One line for a debug console: status, throughput, one backlog number, "failed" (failures and
/// timeouts together, [`TelemetryStats::failed_attempts`]), one loss number, then the loss
/// breakdown.
impl std::fmt::Display for TelemetryStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}, sent {} ({} B), backlog {}, failed {}, lost {} (queue {}, cache {}, expired {}, \
             corrupt {}, invalid {}, rejected {}, oversized {}, throttled {}, rate-limited {}, \
             disabled {}, purged {}; unauthorized {}, holds capped {}, write errors {})",
            self.status,
            self.uploads_sent,
            self.upload_bytes,
            self.cached_batches,
            self.failed_attempts(),
            self.dropped,
            self.dropped_queue_full,
            self.dropped_cache_full + self.dropped_cache_error,
            self.dropped_expired,
            self.dropped_corrupt,
            self.dropped_invalid,
            self.dropped_rejected,
            self.dropped_oversized,
            self.dropped_throttled,
            self.dropped_rate_limited,
            self.dropped_disabled,
            self.dropped_purged,
            self.uploads_unauthorized,
            self.holds_capped,
            self.cache_write_errors,
        )
    }
}

impl TelemetryStats {
    /// Upload attempts that did not get through: `upload_failures` and `upload_timeouts`
    /// together, the "failed" of the `Display` line and of the report's body.
    pub fn failed_attempts(&self) -> u64 {
        self.upload_failures + self.upload_timeouts
    }

    pub(crate) fn new(snapshot: Snapshot, cached_batches: u64, status: TelemetryStatus) -> Self {
        Self {
            status,
            dropped: snapshot.dropped(),
            dropped_queue_full: snapshot.queue_full,
            dropped_cache_error: snapshot.cache_error,
            dropped_cache_full: snapshot.cache_full,
            dropped_expired: snapshot.expired,
            dropped_corrupt: snapshot.corrupt,
            dropped_invalid: snapshot.invalid,
            dropped_rejected: snapshot.rejected,
            dropped_oversized: snapshot.oversized,
            dropped_throttled: snapshot.throttled,
            dropped_disabled: snapshot.disabled,
            dropped_rate_limited: snapshot.rate_limited,
            dropped_purged: snapshot.purged,
            uploads_sent: snapshot.uploads_sent,
            upload_bytes: snapshot.upload_bytes,
            upload_failures: snapshot.upload_failures,
            upload_timeouts: snapshot.upload_timeouts,
            uploads_unauthorized: snapshot.auth_denied,
            holds_capped: snapshot.hold_cap_hits,
            cache_write_errors: snapshot.cache_write_errors,
            cached_batches,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AttributeValue;

    /// A count past `i64::MAX` reaches the report as `i64::MAX`, never as a negative number.
    #[test]
    fn report_counts_saturate_instead_of_wrapping() {
        let snapshot = Snapshot { upload_failures: u64::MAX, ..Snapshot::default() };
        let report = snapshot.report(u64::MAX);
        let value =
            |key: &str| report.attributes.iter().find(|a| a.key == key).map(|a| a.value.clone());
        assert_eq!(value("lk.telemetry.uploads.failed"), Some(AttributeValue::Int(i64::MAX)));
        assert_eq!(value("lk.telemetry.cache.batches"), Some(AttributeValue::Int(i64::MAX)));
    }
}
