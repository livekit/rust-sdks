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
//! `TelemetryTransport` (e.g. a URLSession/OkHttp POST), then `emit` from any thread and push
//! `DeviceState` changes as the OS reports them. The exporter runs on the global runtime;
//! `shutdown` flushes within `export_timeout_ms`.

use std::sync::Arc;

use livekit_telemetry::{
    AttributeValue, DeviceState, RtcStatsSample, TelemetryConfig, TelemetryEvent, TelemetryStats,
    TelemetryTransport,
};

/// Telemetry pipeline: buffer, batch, cache and export events as OTLP.
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

    /// Report the device state (thermal, low power, foreground/background). Emits the matching
    /// `lk.device.*.changed` events and adapts the export cadence.
    pub fn set_device_state(&self, state: DeviceState) {
        self.0.set_device_state(state);
    }

    /// Set (or, with `None`, remove) a session-wide attribute attached to every record from now
    /// on: `lk.room.sid`, `lk.participant.identity`, or the app's own correlation ids.
    pub fn set_attribute(&self, key: String, value: Option<AttributeValue>) {
        self.0.set_attribute(&key, value);
    }

    /// Push one `getStats()` reading for a track; windowed on device into `lk.rtc.stats.sample`.
    pub fn record_stats(&self, sample: RtcStatsSample) {
        self.0.record_stats(sample);
    }

    /// Export everything queued and wait for the transport.
    pub async fn flush(&self) {
        self.0.flush().await;
    }

    /// Flush, then stop exporting. Bounded by `export_timeout_ms`.
    pub async fn shutdown(&self) {
        self.0.shutdown().await;
    }

    /// Pipeline health: drops by reason, uploads, cached batches.
    pub fn stats(&self) -> TelemetryStats {
        self.0.stats()
    }
}
