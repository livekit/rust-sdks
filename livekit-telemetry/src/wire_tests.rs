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

//! Every name and enum value the core sends, frozen. The names come from the protocol's registry
//! (`livekit_protocol::semconv`), so a registry change that renames one fails here before it
//! reaches a dashboard.

use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use prost::Message;

use crate::{
    proto::opentelemetry::proto::{
        collector::{logs::v1::ExportLogsServiceRequest, trace::v1::ExportTraceServiceRequest},
        common::v1::{any_value::Value, KeyValue},
        trace::v1::span::SpanKind,
    },
    stats::Snapshot,
    telemetry::tests::{gunzip, start_with_collector, test_config, FakeTransport},
    AppState, AudioOutput, AudioRouteReason, CaptureDevice, CaptureFailure, DeviceEvent,
    DeviceState, DisconnectReason, LogRecord, LogSource, MemoryPressure, NetworkType,
    ReconnectReason, RoomIdentity, RtcStatsSample, Scope, Sdk, Severity, SpanName, SpanOutcome,
    SpanStep, SpanTrack, StreamDirection, TelemetryConfig, TelemetryResource, ThermalState,
    TrackKind, TrackSource,
};

/// On every record and span of a session; listed once rather than under each record.
const SESSION_KEYS: &[&str] = &[
    "session.id",
    "otel.event.name",
    "lk.room_sid",
    "lk.pii.room_name",
    "lk.participant_id",
    "lk.pii.participant_identity",
];

/// Keys whose values the core picks from a closed set: their values are pinned too.
const ENUM_KEYS: &[&str] = &[
    "service.name",
    "telemetry.sdk.language",
    "lk.device.thermal.state",
    "lk.device.app_state",
    "lk.device.memory.pressure",
    "network.connection.type",
    "lk.device.audio_route.reason",
    "lk.device.audio_route.outputs",
    "lk.device.audio.interruption",
    "lk.device.capture.device",
    "lk.device.capture.reason",
    "lk.track.kind",
    "lk.track.direction",
    "lk.track_source",
    "lk.reconnect.reason",
    "lk.reconnect.mode",
    "lk.disconnect_reason",
    "lk.outcome",
    "error.type",
    "lk.log.source",
];

/// What the core sends: one line per record, span and checkpoint list, plus the values of every
/// [`ENUM_KEYS`] key. A line changes only when a name or value on the wire does.
const PINNED: &str = "\
checkpoints lk.connect: answer_sent | engine | join_recv | offer_sent | pc_connected | pc_created \
    | room_connected | signal | ws_open
checkpoints lk.reconnect: attempt 1 quick | attempt 2 full | attempt 3 quick | attempt 4 full \
    | attempt 5 quick | attempt 6 full | attempt 7 quick | attempt 8 full
checkpoints lk.subscribe: first_media | subscribed
record -: code.file.path | code.function.name | code.line.number | lk.log.logger | lk.log.source
record lk.device.app_state.changed: lk.device.app_state
record lk.device.audio.interruption: lk.device.audio.interruption
record lk.device.audio_route.changed: lk.device.audio_route.outputs | lk.device.audio_route.reason
record lk.device.battery.changed: lk.device.battery.charging | lk.device.battery.level
record lk.device.capture.failed: lk.device.capture.device | lk.device.capture.reason
record lk.device.low_power.changed: lk.device.low_power.enabled
record lk.device.memory.changed: lk.device.memory.pressure
record lk.device.network.changed: lk.device.network.constrained | lk.device.network.expensive \
    | network.connection.type
record lk.device.thermal.changed: lk.device.thermal.state
record lk.room.disconnected: lk.disconnect_reason
record lk.rtc.stats.sample: lk.rtc.audio_level.avg | lk.rtc.audio_level.max \
    | lk.rtc.audio_level.min | lk.rtc.bytes | lk.rtc.codec | lk.rtc.concealed_samples \
    | lk.rtc.concealment_events | lk.rtc.fps.avg | lk.rtc.fps.max | lk.rtc.fps.min \
    | lk.rtc.freeze_count | lk.rtc.freezes_duration_ms | lk.rtc.interruption_count \
    | lk.rtc.interruptions_duration_ms | lk.rtc.jitter_buffer_delay_ms \
    | lk.rtc.jitter_buffer_emitted_count | lk.rtc.jitter_ms.avg | lk.rtc.jitter_ms.max \
    | lk.rtc.jitter_ms.min | lk.rtc.packets | lk.rtc.packets_lost | lk.rtc.pause_count \
    | lk.rtc.pauses_duration_ms | lk.rtc.quality_limitation.bandwidth_ms \
    | lk.rtc.quality_limitation.cpu_ms | lk.rtc.quality_limitation.other_ms | lk.rtc.rtt_ms.avg \
    | lk.rtc.rtt_ms.max | lk.rtc.rtt_ms.min | lk.rtc.samples | lk.rtc.silent_concealed_samples \
    | lk.rtc.window_ms | lk.track.direction | lk.track.kind | lk.track_sid
record lk.telemetry.report: lk.telemetry.cache.batches | lk.telemetry.cache.write_errors \
    | lk.telemetry.dropped.cache_error | lk.telemetry.dropped.cache_full \
    | lk.telemetry.dropped.corrupt | lk.telemetry.dropped.expired | lk.telemetry.dropped.invalid \
    | lk.telemetry.dropped.oversized | lk.telemetry.dropped.queue_full \
    | lk.telemetry.dropped.rate_limited | lk.telemetry.dropped.rejected \
    | lk.telemetry.dropped.throttled | lk.telemetry.holds.capped | lk.telemetry.uploads.bytes \
    | lk.telemetry.uploads.failed | lk.telemetry.uploads.sent | lk.telemetry.uploads.timeouts \
    | lk.telemetry.uploads.unauthorized
resource: device.model.identifier | os.name | os.version | service.name | service.version \
    | telemetry.sdk.language | telemetry.sdk.name | telemetry.sdk.version
session: lk.participant_id | lk.pii.participant_identity | lk.pii.room_name | lk.room_sid \
    | otel.event.name | session.id
severity -: ERROR
severity custom.checkout: INFO
severity lk.device.app_state.changed: INFO
severity lk.device.audio.interruption: INFO
severity lk.device.audio_route.changed: INFO
severity lk.device.battery.changed: INFO
severity lk.device.capture.failed: WARN
severity lk.device.low_power.changed: INFO
severity lk.device.memory.changed: INFO
severity lk.device.network.changed: INFO
severity lk.device.thermal.changed: INFO
severity lk.room.disconnected: INFO | WARN
severity lk.rtc.stats.sample: INFO
severity lk.telemetry.report: INFO
span lk.connect SPAN_KIND_CLIENT: lk.connect.attempt | lk.outcome
span lk.publish SPAN_KIND_INTERNAL: lk.outcome | lk.track.kind | lk.track_sid | lk.track_source
span lk.reconnect SPAN_KIND_CLIENT: lk.outcome | lk.reconnect.attempts | lk.reconnect.mode \
    | lk.reconnect.reason
span lk.subscribe SPAN_KIND_INTERNAL: error.type | lk.outcome | lk.pii.remote_participant_identity \
    | lk.track.kind | lk.track_sid | lk.track_source
value error.type: timed_out
value lk.device.app_state: background | foreground
value lk.device.audio.interruption: began | ended
value lk.device.audio_route.outputs: air_play | bluetooth | car_audio | hdmi | other | receiver \
    | speaker | usb | wired_headset
value lk.device.audio_route.reason: category_change | new_device | no_suitable_route \
    | old_device_unavailable | override | route_configuration_change | unknown | wake_from_sleep
value lk.device.capture.device: camera | microphone | screen_share
value lk.device.capture.reason: disconnected | in_use | not_found | other | permission_denied
value lk.device.memory.pressure: critical | normal | warning
value lk.device.thermal.state: critical | fair | nominal | serious
value lk.disconnect_reason: AGENT_ERROR | CLIENT_INITIATED | CONNECTION_TIMEOUT \
    | DUPLICATE_IDENTITY | JOIN_FAILURE | MEDIA_FAILURE | MIGRATION | PARTICIPANT_REMOVED \
    | RECONNECT_FAILED | ROOM_CLOSED | ROOM_DELETED | SERVER_SHUTDOWN | SIGNAL_CLOSE \
    | SIP_TRUNK_FAILURE | STATE_MISMATCH | UNKNOWN_REASON | USER_REJECTED | USER_UNAVAILABLE
value lk.log.source: ffi | sdk | webrtc
value lk.outcome: cancelled | error | ok
value lk.reconnect.mode: full | quick
value lk.reconnect.reason: debug | network_changed | publisher_failed | signal_disconnected \
    | subscriber_failed | switch_candidate | transport_failed | unknown
value lk.track.direction: inbound | outbound
value lk.track.kind: audio | video
value lk.track_source: SOURCE_CAMERA | SOURCE_MICROPHONE | SOURCE_SCREENSHARE \
    | SOURCE_SCREENSHARE_AUDIO | SOURCE_UNKNOWN
value network.connection.type: bluetooth | cell | other | unavailable | unknown | vpn | wifi \
    | wired
value service.name: livekit-client-swift
value telemetry.sdk.language: rust
";

#[tokio::test(start_paused = true)]
async fn every_name_and_value_on_the_wire_is_pinned() {
    let transport = FakeTransport::scripted([]);
    let config = TelemetryConfig {
        sdk: Some(TelemetryResource {
            sdk: Sdk::Swift,
            sdk_version: "2.17.0".into(),
            os_name: "iOS".into(),
            os_version: "18.5".into(),
            device_model: Some("iPhone17,1".into()),
        }),
        // Every variant below is a discrete event: none may meet the flood guard.
        max_events_per_10min: 10_000,
        ..test_config()
    };
    let telemetry = start_with_collector(config, transport.clone());

    device_states(&telemetry);
    for reason in [
        AudioRouteReason::NewDevice,
        AudioRouteReason::OldDeviceUnavailable,
        AudioRouteReason::CategoryChange,
        AudioRouteReason::Override,
        AudioRouteReason::WakeFromSleep,
        AudioRouteReason::NoSuitableRoute,
        AudioRouteReason::RouteConfigurationChange,
        AudioRouteReason::Unknown,
    ] {
        use AudioOutput::*;
        let outputs =
            vec![Speaker, Receiver, WiredHeadset, Bluetooth, CarAudio, AirPlay, Hdmi, Usb, Other];
        telemetry.device_event(DeviceEvent::AudioRouteChanged { outputs, reason });
    }
    for began in [true, false] {
        telemetry.device_event(DeviceEvent::AudioInterruption { began });
    }
    for (device, reason) in [
        (CaptureDevice::Camera, CaptureFailure::PermissionDenied),
        (CaptureDevice::Microphone, CaptureFailure::NotFound),
        (CaptureDevice::ScreenShare, CaptureFailure::InUse),
        (CaptureDevice::Camera, CaptureFailure::Disconnected),
        (CaptureDevice::Camera, CaptureFailure::Other),
    ] {
        telemetry.device_event(DeviceEvent::CaptureFailed { device, reason });
    }
    for source in [LogSource::Sdk, LogSource::Ffi, LogSource::WebRtc] {
        telemetry.log(LogRecord {
            severity: Severity::Error,
            source,
            body: "failed".into(),
            logger: Some("Room".into()),
            function: Some("connect()".into()),
            file: Some("Room.swift".into()),
            line: Some(42),
            timestamp_ns: None,
            span_id: None,
        });
    }
    let every_counter = Snapshot {
        queue_full: 1,
        cache_error: 1,
        cache_full: 1,
        expired: 1,
        corrupt: 1,
        invalid: 1,
        rejected: 1,
        oversized: 1,
        throttled: 1,
        disabled: 1,
        rate_limited: 1,
        purged: 1,
        uploads_sent: 1,
        upload_bytes: 1,
        upload_failures: 1,
        upload_timeouts: 1,
        auth_denied: 1,
        hold_cap_hits: 1,
        cache_write_errors: 1,
    };
    telemetry.emit(every_counter.report(1));

    let room = telemetry.begin_scope();
    room.set_room(RoomIdentity {
        sid: Some("RM_1".into()),
        name: Some("standup".into()),
        participant_sid: Some("PA_1".into()),
        participant_identity: Some("alice".into()),
    });
    room.emit_custom("checkout", Vec::new());
    spans(&room).await;
    stats(&room);
    telemetry.flush().await;
    for reason in (0..=16).map(DisconnectReason::from).chain([DisconnectReason::ReconnectFailed]) {
        telemetry.begin_scope().disconnected(reason);
    }
    telemetry.shutdown().await;

    assert_eq!(wire(&transport), PINNED);
}

/// Every value of every device state field, ending at rest so nothing holds the uploads.
fn device_states(telemetry: &crate::Telemetry) {
    let mut state = DeviceState {
        low_power_mode: Some(true),
        battery_level: Some(50),
        network_expensive: true,
        ..Default::default()
    };
    telemetry.set_device_state(state);
    for thermal in [ThermalState::Fair, ThermalState::Serious, ThermalState::Critical] {
        telemetry.set_device_state(DeviceState { thermal, ..state });
    }
    for memory in [MemoryPressure::Warning, MemoryPressure::Critical] {
        telemetry.set_device_state(DeviceState { memory, ..state });
    }
    for network in [
        NetworkType::Unknown,
        NetworkType::Wifi,
        NetworkType::Cell,
        NetworkType::Wired,
        NetworkType::Vpn,
        NetworkType::Bluetooth,
        NetworkType::Other,
        NetworkType::Unavailable,
    ] {
        telemetry.set_device_state(DeviceState { network, ..state });
    }
    state = DeviceState {
        thermal: ThermalState::Nominal,
        low_power_mode: Some(false),
        app_state: AppState::Background,
        memory: MemoryPressure::Normal,
        network: NetworkType::Wifi,
        network_expensive: false,
        network_constrained: false,
        battery_level: Some(50),
        battery_charging: true,
    };
    telemetry.set_device_state(state);
    telemetry.set_device_state(DeviceState { app_state: AppState::Foreground, ..state });
}

/// Every typed span, checkpoint, reconnect reason, track source and outcome.
async fn spans(room: &Scope) {
    let connect = room.start(SpanName::Connect, None);
    for step in [
        SpanStep::WsOpen,
        SpanStep::Signal,
        SpanStep::JoinRecv,
        SpanStep::PcCreated,
        SpanStep::OfferSent,
        SpanStep::AnswerSent,
        SpanStep::Engine,
        SpanStep::PcConnected,
        SpanStep::RoomConnected,
    ] {
        connect.step(step);
    }
    connect.end(SpanOutcome::Ok, None);
    for (n, reason) in [
        ReconnectReason::SignalDisconnected,
        ReconnectReason::PublisherFailed,
        ReconnectReason::SubscriberFailed,
        ReconnectReason::TransportFailed,
        ReconnectReason::SwitchCandidate,
        ReconnectReason::NetworkChanged,
        ReconnectReason::Debug,
        ReconnectReason::Unknown,
    ]
    .into_iter()
    .enumerate()
    {
        let reconnect = room.start(SpanName::Reconnect { reason }, None);
        reconnect.step(SpanStep::Attempt { number: n as u32 + 1, full: n % 2 == 1 });
        reconnect.end(SpanOutcome::Ok, None);
    }
    let track = |sid: &str, kind, source| SpanTrack {
        sid: Some(sid.into()),
        kind,
        source,
        remote_identity: Some("bob".into()),
    };
    for (sid, source) in [
        ("TR_cam", TrackSource::Camera),
        ("TR_mic", TrackSource::Microphone),
        ("TR_screen", TrackSource::ScreenShare),
        ("TR_screen_audio", TrackSource::ScreenShareAudio),
        ("TR_other", TrackSource::Unknown),
    ] {
        let publish = room.start(SpanName::Publish, None);
        publish
            .set_track(SpanTrack { remote_identity: None, ..track(sid, TrackKind::Audio, source) });
        publish.end(SpanOutcome::Ok, None);
    }
    // Ok at first media, cancelled when the track ends first, timed out without media.
    room.subscribe_started(track("TR_ok", TrackKind::Video, TrackSource::Camera));
    room.subscribed(track("TR_ok", TrackKind::Video, TrackSource::Camera));
    let mut media = RtcStatsSample::new("TR_ok", TrackKind::Video, StreamDirection::Inbound);
    media.bytes = Some(1_500);
    room.record_stats(media);
    room.subscribe_started(track("TR_gone", TrackKind::Audio, TrackSource::Microphone));
    room.track_ended("TR_gone");
    room.subscribe_started(track("TR_silent", TrackKind::Audio, TrackSource::Microphone));
    tokio::time::sleep(Scope::SUBSCRIBE_TIMEOUT + Duration::from_secs(1)).await;
}

/// One window per kind and direction, every counter and gauge set.
fn stats(room: &Scope) {
    for (sid, kind, direction) in [
        ("TR_av_in", TrackKind::Audio, StreamDirection::Inbound),
        ("TR_av_out", TrackKind::Audio, StreamDirection::Outbound),
        ("TR_vv_in", TrackKind::Video, StreamDirection::Inbound),
        ("TR_vv_out", TrackKind::Video, StreamDirection::Outbound),
    ] {
        room.record_stats(RtcStatsSample {
            codec: Some("opus".into()),
            bytes: Some(1),
            packets: Some(1),
            packets_lost: Some(1),
            freeze_count: Some(1),
            freezes_duration_ms: Some(1),
            concealed_samples: Some(1),
            concealment_events: Some(1),
            jitter_buffer_delay_ms: Some(1),
            jitter_buffer_emitted_count: Some(1),
            quality_limitation_bandwidth_ms: Some(1),
            quality_limitation_cpu_ms: Some(1),
            quality_limitation_other_ms: Some(1),
            pause_count: Some(1),
            pauses_duration_ms: Some(1),
            silent_concealed_samples: Some(1),
            interruption_count: Some(1),
            interruptions_duration_ms: Some(1),
            jitter_ms: Some(1.0),
            rtt_ms: Some(1.0),
            frames_per_second: Some(1.0),
            audio_level: Some(1.0),
            ..RtcStatsSample::new(sid, kind, direction)
        });
    }
}

type Lines = BTreeMap<String, BTreeSet<String>>;

/// What went out, one line per signal: its keys, severities, checkpoints, and the values of
/// [`ENUM_KEYS`].
fn wire(transport: &FakeTransport) -> String {
    let mut lines = Lines::new();
    for request in transport.sent() {
        let body = gunzip(&request.body);
        if request.url.contains("traces") {
            let decoded = ExportTraceServiceRequest::decode(&body[..]).expect("otlp traces");
            for resource_spans in decoded.resource_spans {
                let resource = resource_spans.resource.unwrap_or_default();
                add_attributes(&mut lines, "resource", &resource.attributes);
                for span in resource_spans.scope_spans.into_iter().flat_map(|s| s.spans) {
                    let kind = SpanKind::try_from(span.kind).expect("kind").as_str_name();
                    let line = format!("span {} {kind}", span.name);
                    add_attributes(&mut lines, &line, &span.attributes);
                    for event in &span.events {
                        add(&mut lines, format!("checkpoints {}", span.name), &event.name);
                    }
                }
            }
        } else {
            let decoded = ExportLogsServiceRequest::decode(&body[..]).expect("otlp logs");
            for resource_logs in decoded.resource_logs {
                let resource = resource_logs.resource.unwrap_or_default();
                add_attributes(&mut lines, "resource", &resource.attributes);
                for record in resource_logs.scope_logs.into_iter().flat_map(|s| s.log_records) {
                    let name = Some(record.event_name.as_str()).filter(|n| !n.is_empty());
                    let name = name.unwrap_or("-");
                    add_attributes(&mut lines, &format!("record {name}"), &record.attributes);
                    add(&mut lines, format!("severity {name}"), &record.severity_text);
                }
            }
        }
    }
    lines
        .into_iter()
        .map(|(line, items)| format!("{line}: {}\n", Vec::from_iter(items).join(" | ")))
        .collect()
}

fn add(lines: &mut Lines, line: String, item: &str) {
    lines.entry(line).or_default().insert(item.to_owned());
}

/// The keys under `line` (a session's under `session`), and the values of [`ENUM_KEYS`].
fn add_attributes(lines: &mut Lines, line: &str, attributes: &[KeyValue]) {
    for kv in attributes {
        let session = SESSION_KEYS.contains(&kv.key.as_str());
        add(lines, if session { "session".into() } else { line.into() }, &kv.key);
        let Some(Value::StringValue(value)) = kv.value.as_ref().and_then(|v| v.value.as_ref())
        else {
            continue;
        };
        if ENUM_KEYS.contains(&kv.key.as_str()) {
            // The audio route's outputs are one comma-separated value.
            for value in value.split(',') {
                add(lines, format!("value {}", kv.key), value);
            }
        }
    }
}
