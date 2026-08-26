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

//! Sends a single `lk.ping` event to an OTLP/HTTP collector (default: local grafana/otel-lgtm).

use std::{env, sync::Arc};

use livekit_telemetry::{Attribute, NetTransport, Telemetry, TelemetryConfig, TelemetryEvent};

#[tokio::main]
async fn main() {
    env_logger::init();
    let endpoint =
        env::var("LK_OTLP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4318/v1/logs".to_owned());

    let mut config = TelemetryConfig::new(&endpoint);
    config.resource.push(Attribute::new("service.name", "telemetry_ping"));
    config.resource.push(Attribute::new("os.name", env::consts::OS));
    // Optional on-disk cache: run once with the collector down, once with it up.
    config.storage_dir = env::var("LK_TELEMETRY_DIR").ok();
    let transport = NetTransport::from_registry().expect("livekit-net has no HTTP client");

    let (telemetry, exporter) = Telemetry::new(config, Arc::new(transport));
    tokio::spawn(exporter.run());

    telemetry.emit(
        TelemetryEvent::new("lk.ping")
            .with_body("hello from livekit-telemetry")
            .with_attribute("lk.ping.seq", 1i64),
    );
    telemetry.shutdown().await;
    println!("sent lk.ping to {endpoint} (dropped: {})", telemetry.dropped_count());
}
