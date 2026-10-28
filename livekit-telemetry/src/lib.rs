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

/// Event data model: what SDKs push in.
mod event;

/// Bounded in-memory queue between `emit` and the exporter.
#[expect(
    dead_code,
    reason = "consumed by the pipeline (`Telemetry`, `Exporter`), not in place yet"
)]
mod store;

/// Pipeline health counters and the `lk.telemetry.report` event.
#[expect(
    dead_code,
    reason = "consumed by the pipeline (`Telemetry`, `Exporter`), not in place yet"
)]
mod stats;

/// Spans: one attempt at an operation, with explicit handles across the FFI.
#[expect(
    dead_code,
    reason = "consumed by the pipeline (`Telemetry`, `Exporter`), not in place yet"
)]
mod scope;
#[expect(
    dead_code,
    reason = "consumed by the pipeline (`Telemetry`, `Exporter`), not in place yet"
)]
mod span;

/// OTLP/HTTP protobuf encoding of a batch.
#[expect(
    dead_code,
    reason = "consumed by the pipeline (`Telemetry`, `Exporter`), not in place yet"
)]
mod otlp;

/// OTLP protobuf types (re-exported from `opentelemetry-proto`).
mod proto;

pub use event::*;
pub use span::SpanOutcome;
pub use stats::{TelemetryStats, TelemetryStatus};

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

/// Warnings logged by the tests, so a test can assert the ones it caused.
#[cfg(test)]
pub(crate) mod test_log {
    use std::sync::{Mutex, Once};
    use std::thread::{self, ThreadId};

    static LINES: Mutex<Vec<(ThreadId, String)>> = Mutex::new(Vec::new());

    struct Capture;

    impl log::Log for Capture {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Warn
        }

        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                let line = (thread::current().id(), record.args().to_string());
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
