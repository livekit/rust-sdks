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

#![doc = include_str!("../README.md")]

/// Event data model: what SDKs push in.
mod event;

/// Bounded in-memory queue between `emit` and the exporter.
mod store;

/// Pipeline health counters and the `lk.telemetry.report` event.
mod stats;

/// Host-reported device state and the cadence policy derived from it.
mod device;

/// RTC stats samples and their on-device windowing.
mod rtc;

/// Session identity: the trace id, attributes and route of one Room.
mod scope;
/// Spans: one attempt at an operation, with explicit handles across the FFI.
mod span;

/// Batch exporter actor: timer, OTLP encoding, retry policy.
mod exporter;

/// OTLP/HTTP protobuf encoding of a batch.
mod otlp;

/// Queue of encoded batches between the exporter and the transport (memory or disk).
mod cache;

/// OTLP protobuf types (re-exported from `opentelemetry-proto`).
mod proto;

/// Transport seam: how encoded batches leave the device.
mod transport;

/// Where batches go: server URL + token → ingest URL, grant, expiry, per-project routing.
mod destination;

/// Entry point and configuration.
mod telemetry;
mod trace;

pub use cache::{BatchCache, FileCache, MemoryCache};
pub use destination::ENDPOINT_OVERRIDE_ENV;
pub use device::*;
pub use event::*;
pub use exporter::Exporter;
pub use rtc::{RtcStat, RtcStatsSample, StreamDirection, TrackKind};
pub use scope::{DisconnectReason, RoomIdentity, Scope};
pub use span::SpanOutcome;
pub use stats::{TelemetryStats, TelemetryStatus};
pub use telemetry::*;
pub use trace::*;
#[cfg(feature = "net")]
pub use transport::NetTransport;
pub use transport::{ExportError, ExportRequest, ExportResponse, TelemetryTransport};

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

/// Warnings logged by the tests, so a test can assert the ones it caused.
#[cfg(test)]
pub(crate) mod test_log {
    use std::cell::RefCell;
    use std::sync::{Mutex, Once};
    use std::thread::{self, ThreadId};

    static LINES: Mutex<Vec<(ThreadId, String)>> = Mutex::new(Vec::new());

    thread_local! {
        /// Asked at every line this thread logs; a `true` marks the line `held: `.
        static HELD: RefCell<Option<Box<dyn Fn() -> bool>>> = const { RefCell::new(None) };
    }

    struct Capture;

    impl log::Log for Capture {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Warn
        }

        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                let held = HELD.with(|held| held.borrow().as_ref().is_some_and(|held| held()));
                let line = format!("{}{}", if held { "held: " } else { "" }, record.args());
                let line = (thread::current().id(), line);
                LINES.lock().unwrap_or_else(|e| e.into_inner()).push(line);
            }
        }

        fn flush(&self) {}
    }

    /// Start capturing; `log` takes one logger per process, so the first test installs it.
    pub fn capture() {
        static INSTALL: Once = Once::new();
        INSTALL.call_once(|| {
            log::set_logger(&Capture).expect("the only logger in tests");
            log::set_max_level(log::LevelFilter::Warn);
        });
    }

    /// Mark every line this thread logs while `held` returns `true` (a lock taken, say) with a
    /// `held: ` prefix: the host logger would have run at that moment.
    pub fn mark_while(held: impl Fn() -> bool + 'static) {
        HELD.with(|slot| *slot.borrow_mut() = Some(Box::new(held)));
    }

    /// The warnings logged on this thread since the last call: this test's own, since every
    /// test runs on a thread of its own.
    pub fn take() -> Vec<String> {
        let me = thread::current().id();
        let mut lines = LINES.lock().unwrap_or_else(|e| e.into_inner());
        let (mine, others): (Vec<_>, Vec<_>) =
            std::mem::take(&mut *lines).into_iter().partition(|(id, _)| *id == me);
        *lines = others;
        mine.into_iter().map(|(_, line)| line).collect()
    }
}
