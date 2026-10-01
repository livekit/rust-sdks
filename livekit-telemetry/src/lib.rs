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

// Internals the pipeline (`Telemetry`, `Exporter`) consumes once it is in place.
#![allow(dead_code)]

/// Event data model: what SDKs push in.
mod event;

/// Bounded in-memory queue between `emit` and the exporter.
mod store;

/// Pipeline health counters and the `lk.telemetry.report` event.
mod stats;

/// Spans: one attempt at an operation, with explicit handles across the FFI.
mod scope;
mod span;

/// OTLP/HTTP protobuf encoding of a batch.
mod otlp;

/// OTLP protobuf types (re-exported from `opentelemetry-proto`).
mod proto;

/// Transport seam: how encoded batches leave the device.
mod transport;

/// Where batches go: server URL + token → ingest URL, grant, expiry, per-project routing.
mod destination;

pub use destination::ENDPOINT_OVERRIDE_ENV;
pub use event::*;
pub use span::SpanOutcome;
pub use stats::{TelemetryStats, TelemetryStatus};
pub use transport::*;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();
