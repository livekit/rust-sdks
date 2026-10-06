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

//! Records a small session through the telemetry core — an `lk.connect` span with its
//! checkpoints, an `lk.publish`, an `lk.subscribe` ended by first media, and a custom event —
//! and exports it.
//!
//! - A local OpenTelemetry collector: `LK_TELEMETRY_ENDPOINT=http://localhost:4318` (the core's
//!   test-only override; any server URL and token are accepted).
//! - A LiveKit Cloud project: `LK_URL=wss://<project>.livekit.cloud` and `LK_TOKEN=<participant
//!   token with the observability grant>`.

use std::{env, sync::Arc};

use livekit_telemetry::{
    Attribute, NetTransport, RoomIdentity, RtcStatsSample, SpanName, SpanOutcome, SpanStep,
    SpanTrack, StreamDirection, Telemetry, TelemetryConfig, TrackKind, TrackSource,
    ENDPOINT_OVERRIDE_ENV,
};

#[tokio::main]
async fn main() {
    env_logger::init();
    let url = env::var("LK_URL").unwrap_or_else(|_| "ws://localhost:7880".to_owned());
    let token = env::var("LK_TOKEN").unwrap_or_default();
    if env::var(ENDPOINT_OVERRIDE_ENV).is_err() && env::var("LK_URL").is_err() {
        eprintln!("set {ENDPOINT_OVERRIDE_ENV}=http://localhost:4318, or LK_URL and LK_TOKEN");
        return;
    }

    let config = TelemetryConfig {
        resource: vec![
            Attribute::new("service.name", "telemetry_ping"),
            Attribute::new("os.name", env::consts::OS),
        ],
        // Optional on-disk cache: run once with the collector down, once with it up.
        storage_dir: env::var("LK_TELEMETRY_DIR").ok(),
        ..Default::default()
    };
    let transport = NetTransport::from_registry().expect("livekit-net has no HTTP client");
    let (telemetry, exporter) = Telemetry::new(config, Arc::new(transport));
    tokio::spawn(exporter.run());

    let room = telemetry.begin_scope();
    room.set_server(&url, &token);
    room.set_room(RoomIdentity { name: Some("telemetry-ping".into()), ..Default::default() });

    let connect = room.start(SpanName::Connect, None);
    for step in [SpanStep::WsOpen, SpanStep::Signal, SpanStep::JoinRecv, SpanStep::PcCreated] {
        connect.step(step);
    }
    connect.end(SpanOutcome::Ok, None);

    let microphone = SpanTrack {
        sid: Some("TR_ping_mic".into()),
        kind: TrackKind::Audio,
        source: TrackSource::Microphone,
        remote_identity: None,
    };
    let publish = room.start(SpanName::Publish, None);
    publish.set_track(microphone);
    publish.end(SpanOutcome::Ok, None);

    let remote = SpanTrack {
        sid: Some("TR_ping_remote".into()),
        kind: TrackKind::Video,
        source: TrackSource::Camera,
        remote_identity: Some("bob".into()),
    };
    room.subscribe_started(remote.clone());
    room.subscribed(remote);
    let mut media =
        RtcStatsSample::new("TR_ping_remote", TrackKind::Video, StreamDirection::Inbound);
    media.bytes = Some(1_500);
    room.record_stats(media); // first media: the lk.subscribe span ends ok

    room.emit_custom("ping", vec![Attribute::new("seq", 1i64)]);
    telemetry.shutdown().await;
    println!("trace {} — {}", room.trace_id(), telemetry.stats());
}
