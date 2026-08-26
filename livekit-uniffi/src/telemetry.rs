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

//! Client telemetry core from the [`livekit-telemetry`] crate.
//!
//! FFI clients construct one [`Telemetry`] per pipeline with a host-implemented
//! `TelemetryTransport` (e.g. a URLSession/OkHttp POST), then `emit` from any thread. The
//! exporter runs on the global runtime; `shutdown` flushes within `export_timeout_ms`.

use std::sync::Arc;

use livekit_telemetry::{TelemetryConfig, TelemetryEvent, TelemetryTransport};

/// Telemetry pipeline: buffer, batch and export events as OTLP.
#[derive(uniffi::Object)]
pub struct Telemetry(livekit_telemetry::Telemetry);

#[uniffi::export(async_runtime = "tokio")]
impl Telemetry {
    #[uniffi::constructor]
    pub fn new(config: TelemetryConfig, transport: Arc<dyn TelemetryTransport>) -> Arc<Self> {
        let (telemetry, exporter) = livekit_telemetry::Telemetry::new(config, transport);
        crate::runtime::runtime().spawn(exporter.run());
        Arc::new(Self(telemetry))
    }

    /// Queue an event for export. Never blocks; drops the oldest event when the queue is full.
    pub fn emit(&self, event: TelemetryEvent) {
        self.0.emit(event);
    }

    /// Export everything queued and wait for the transport.
    pub async fn flush(&self) {
        self.0.flush().await;
    }

    /// Flush, then stop exporting. Bounded by `export_timeout_ms`.
    pub async fn shutdown(&self) {
        self.0.shutdown().await;
    }

    /// Events dropped so far (queue overflow, rejected/failed exports, disabled collector).
    pub fn dropped_count(&self) -> u64 {
        self.0.dropped_count()
    }
}
