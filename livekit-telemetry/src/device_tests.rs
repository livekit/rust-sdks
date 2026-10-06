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

//! The device contract: what can go wrong on a phone — lifecycle, crashes, disk, network,
//! power, opt-out — simulated with fake transports, fake device state and paused time, and how
//! many records each case can lose. One test per row of the contract table in the PR
//! description.

use std::{
    fs, io,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    cache::temp_dir,
    telemetry::tests::{
        event_names, exported_spans, files_in, offline_forever, records, start, test_config,
        FakeTransport,
    },
    BatchCache, DeviceState, ExportError, ExportRequest, ExportResponse, MemoryCache, NetworkType,
    RtcStatsSample, SpanName, SpanOutcome, StreamDirection, Telemetry, TelemetryConfig,
    TelemetryEvent, TelemetryTransport, TrackKind,
};

fn on_disk(dir: &Path) -> TelemetryConfig {
    TelemetryConfig { storage_dir: Some(dir.to_string_lossy().into_owned()), ..test_config() }
}

/// A pipeline whose exporter task the test can kill, like the OS kills an app.
fn killable(
    config: TelemetryConfig,
    transport: Arc<dyn TelemetryTransport>,
) -> (Telemetry, tokio::task::JoinHandle<()>) {
    let (telemetry, exporter) = Telemetry::new(config, transport);
    telemetry.override_endpoint("http://collector");
    (telemetry, tokio::spawn(exporter.run()))
}

#[tokio::test(start_paused = true)]
async fn defaults_export_and_window_once_a_minute() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start(TelemetryConfig::default(), transport.clone());
    tokio::time::sleep(Duration::from_millis(10)).await; // the start-up replay is behind us
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    telemetry.record_stats(RtcStatsSample::new("TR_1", TrackKind::Audio, StreamDirection::Inbound));
    tokio::time::sleep(Duration::from_secs(59)).await;
    assert!(transport.sent().is_empty(), "nothing before the minute is up");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let names: Vec<String> = transport.sent().iter().flat_map(event_names).collect();
    assert_eq!(transport.sent().len(), 1, "one request a minute");
    assert!(
        names.contains(&"lk.ping".to_owned()) && names.contains(&"lk.rtc.stats.sample".to_owned())
    );
}

/// Killed mid-session: what reached the cache survives; what was still in memory — at most one
/// flush interval of records (≤ `max_queue_size`), open spans, open RTC windows — is lost.
#[tokio::test(start_paused = true)]
async fn a_crash_loses_only_what_was_not_yet_cached() {
    let dir = temp_dir("crash");
    let (telemetry, task) = killable(on_disk(&dir), FakeTransport::scripted(offline_forever()));
    telemetry.emit(TelemetryEvent::new("custom.cached"));
    telemetry.flush().await; // offline: written ahead, kept
    telemetry.emit(TelemetryEvent::new("custom.in_memory"));
    task.abort(); // the OS kills the app before the next tick
    let _ = task.await;

    let transport = FakeTransport::scripted([]);
    let (next, _task) = killable(on_disk(&dir), transport.clone());
    next.flush().await;
    let names: Vec<String> = transport.sent().iter().flat_map(event_names).collect();
    assert!(names.contains(&"custom.cached".to_owned()), "replayed on the next launch");
    assert!(!names.contains(&"custom.in_memory".to_owned()), "lost: never reached the cache");
    let _ = fs::remove_dir_all(&dir);
}

/// Killed while a request is in flight: the collector may have the batch, the cache still does.
/// Delivery is at-least-once; one request is in flight at a time, so at most one batch repeats.
#[tokio::test(start_paused = true)]
async fn a_batch_in_flight_at_a_crash_is_sent_again_once() {
    struct Swallow(Mutex<Vec<ExportRequest>>);
    #[async_trait::async_trait]
    impl TelemetryTransport for Swallow {
        async fn send(&self, request: ExportRequest) -> Result<ExportResponse, ExportError> {
            self.0.lock().expect("lock").push(request);
            std::future::pending().await // the collector got it; the answer never arrives
        }
    }
    let dir = temp_dir("inflight");
    let collector = Arc::new(Swallow(Mutex::new(Vec::new())));
    let (telemetry, task) = killable(on_disk(&dir), collector.clone());
    for _ in 0..3 {
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        tokio::task::yield_now().await;
    }
    tokio::time::sleep(Duration::from_millis(1500)).await;
    task.abort();
    let _ = task.await;
    assert_eq!(collector.0.lock().expect("lock").len(), 1, "one request in flight");

    let transport = FakeTransport::scripted([]);
    let (next, _task) = killable(on_disk(&dir), transport.clone());
    next.flush().await;
    let first = &collector.0.lock().expect("lock")[0];
    let resent = transport.sent().iter().filter(|r| r.body == first.body).count();
    assert_eq!(resent, 1, "the in-flight batch is sent again, exactly once");
    let _ = fs::remove_dir_all(&dir);
}

/// Disk full, directory purged by the OS, read-only volume: batches stay in memory and still
/// upload; only a crash on top of it loses them. The report says it happened.
#[tokio::test(start_paused = true)]
async fn a_full_disk_keeps_batches_in_memory_and_says_so() {
    struct FullDisk;
    impl BatchCache for FullDisk {
        fn push(&self, _: &str, _: &[u8]) -> io::Result<Vec<String>> {
            Err(io::Error::new(io::ErrorKind::StorageFull, "no space left on device"))
        }
        fn pending(&self) -> Vec<String> {
            Vec::new()
        }
        fn replace(&self, _: &str, _: &[(String, Vec<u8>)]) -> io::Result<Vec<String>> {
            Err(io::Error::new(io::ErrorKind::StorageFull, "no space left on device"))
        }
        fn read(&self, _: &str) -> io::Result<Vec<u8>> {
            Err(io::ErrorKind::NotFound.into())
        }
        fn remove(&self, _: &str) -> io::Result<()> {
            Ok(())
        }
        fn clear(&self) -> io::Result<()> {
            Ok(())
        }
    }
    let transport = FakeTransport::scripted([]);
    let (telemetry, exporter) =
        Telemetry::with_cache(test_config(), transport.clone(), Arc::new(FullDisk));
    telemetry.override_endpoint("http://collector");
    tokio::spawn(exporter.run());
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    telemetry.flush().await;
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    telemetry.flush().await;
    let stats = telemetry.stats();
    assert_eq!((stats.uploads_sent, stats.dropped), (2, 0), "nothing lost while the process lives");
    assert_eq!(stats.cache_write_errors, 2);
    let reported = transport.sent().iter().flat_map(records).any(|r| {
        r.event_name == "lk.telemetry.report"
            && r.attributes.iter().any(|a| a.key == "lk.telemetry.cache.write_errors")
    });
    assert!(reported, "the spill is reported");
}

#[tokio::test(start_paused = true)]
async fn corrupt_and_stray_cache_files_are_dropped_and_counted() {
    let dir = temp_dir("corrupt");
    fs::create_dir(&dir).expect("dir");
    let now = crate::event::now_unix_nanos();
    // A real gzip member to damage: truncated (a torn write), and with one bit flipped in its
    // payload (the CRC catches it) — plus bytes that are not gzip at all.
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    std::io::Write::write_all(&mut gz, &[7u8; 4096]).expect("gzip");
    let good = gz.finish().expect("gzip");
    let truncated = &good[..good.len() / 2];
    let mut flipped = good.clone();
    flipped[good.len() / 2] ^= 0x01;
    for (seq, body) in [(1, b"not gzip".as_slice()), (2, truncated), (3, flipped.as_slice())] {
        fs::write(dir.join(format!("{now:020}-{seq:06}-5-l.otlp")), body).expect("write");
    }
    fs::write(dir.join("garbage.otlp"), b"??").expect("write");
    fs::write(dir.join("half-written.tmp"), b"??").expect("write");
    let transport = FakeTransport::scripted([]);
    let (telemetry, _task) = killable(on_disk(&dir), transport.clone());
    telemetry.flush().await;
    assert!(transport.sent().is_empty(), "a corrupt batch never reaches the collector");
    assert_eq!(telemetry.stats().dropped_corrupt, 15, "their records are counted");
    assert_eq!(files_in(&dir), 0, "stray files are cleaned up");
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test(start_paused = true)]
async fn batches_older_than_a_day_are_dropped_and_counted_at_start() {
    let dir = temp_dir("stale");
    fs::create_dir(&dir).expect("dir");
    let day_ago = crate::event::now_unix_nanos() - 25 * 3600 * 1_000_000_000;
    fs::write(dir.join(format!("{day_ago:020}-000001-7-l.otlp")), [0x1f, 0x8b]).expect("write");
    let (telemetry, _task) = killable(on_disk(&dir), FakeTransport::scripted([]));
    telemetry.flush().await;
    assert_eq!(telemetry.stats().dropped_expired, 7);
    assert_eq!(files_in(&dir), 0);
    let _ = fs::remove_dir_all(&dir);
}

/// Offline, Low Data Mode, battery ≤ 10 % unplugged: record, never attempt — no failure is
/// counted and no retry is spent — and ship once the device allows. Offline is a hard hold (no
/// escape, however long); the other two are soft (one batch a minute gets through).
#[tokio::test(start_paused = true)]
async fn device_holds_record_without_attempting() {
    let holds = [
        (DeviceState { network: NetworkType::Unavailable, ..Default::default() }, 600),
        (DeviceState { network_constrained: true, ..Default::default() }, 50),
        (DeviceState { battery_level: Some(5), battery_charging: false, ..Default::default() }, 50),
    ];
    for (hold, secs) in holds {
        let transport = FakeTransport::scripted([]);
        let telemetry = start(test_config(), transport.clone());
        telemetry.set_device_state(hold);
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        tokio::time::sleep(Duration::from_secs(secs)).await;
        assert!(transport.sent().is_empty(), "{hold:?}: nothing attempted for {secs} s");
        let stats = telemetry.stats();
        assert_eq!((stats.upload_failures, stats.dropped), (0, 0), "{hold:?}");
        telemetry.set_device_state(DeviceState { battery_charging: true, ..Default::default() });
        telemetry.flush().await;
        assert!(!transport.sent().is_empty(), "{hold:?}: ships once released");
    }
}

/// Opt-out: collection stops, and the queue, the spans, the RTC windows and every cached batch —
/// on disk included — are deleted without another upload.
#[tokio::test(start_paused = true)]
async fn opting_out_stops_collection_and_purges_everything() {
    let dir = temp_dir("optout");
    let transport = FakeTransport::scripted(offline_forever());
    let (telemetry, _task) = killable(on_disk(&dir), transport.clone());
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    telemetry.flush().await; // offline: one batch on disk
    assert_eq!(files_in(&dir), 1);
    let room = telemetry.begin_scope();
    room.start(SpanName::Publish, None).end(SpanOutcome::Ok, None);
    let _open = room.start(SpanName::Connect, None);
    room.record_stats(RtcStatsSample::new("TR_1", TrackKind::Audio, StreamDirection::Inbound));
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    let attempts = transport.sent().len();

    telemetry.purge().await;
    assert_eq!(files_in(&dir), 0, "the cache is deleted");
    assert_eq!(transport.sent().len(), attempts, "nothing more is uploaded");
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    telemetry.flush().await;
    assert_eq!(files_in(&dir), 0, "and nothing is collected afterwards");
    assert_eq!(telemetry.stats().cached_batches, 0);
    let _ = fs::remove_dir_all(&dir);
}

/// Finding 13: a subscribe that never sees media ends `timed_out` on the core's own clock, and
/// a disconnect after the deadline keeps that outcome instead of `cancelled`.
#[tokio::test(start_paused = true)]
async fn a_subscribe_past_its_deadline_stays_timed_out_through_disconnect() {
    use crate::{DisconnectReason, SpanTrack, TrackSource};
    let transport = FakeTransport::scripted([]);
    // No exporter running: nothing sweeps, the disconnect is the first to look.
    let (telemetry, exporter) = Telemetry::new(test_config(), transport.clone());
    telemetry.override_endpoint("http://collector");
    let room = telemetry.begin_scope();
    let track = |sid: &str| SpanTrack {
        sid: Some(sid.into()),
        kind: TrackKind::Audio,
        source: TrackSource::Microphone,
        remote_identity: None,
    };
    room.subscribe_started(track("TR_late"));
    tokio::time::advance(crate::Scope::SUBSCRIBE_TIMEOUT + Duration::from_secs(1)).await;
    room.subscribe_started(track("TR_fresh"));
    room.disconnected(DisconnectReason::ClientInitiated);
    tokio::spawn(exporter.run());
    telemetry.flush().await;
    let spans = exported_spans(&transport);
    let error_type = |sid: &str| {
        let span = spans
            .iter()
            .find(|s| {
                s.attributes
                    .iter()
                    .any(|a| a.key == "lk.track.sid" && format!("{:?}", a.value).contains(sid))
            })
            .expect("span");
        span.attributes.iter().find(|a| a.key == "lk.outcome").map(|a| format!("{:?}", a.value))
    };
    assert!(error_type("TR_late").is_some_and(|o| o.contains("error")), "timed out");
    assert!(error_type("TR_fresh").is_some_and(|o| o.contains("cancelled")));
}

/// Queue overflow and the flood guard are the two in-memory bounds; both count every record
/// they drop. (The cache's bounds: `cache_eviction_is_counted_as_a_drop`,
/// `a_hold_longer_than_the_cache_reports_what_it_cost`, `file_cache_caps_the_number_of_batches`.)
#[tokio::test(start_paused = true)]
async fn in_memory_bounds_count_every_drop() {
    let config = TelemetryConfig { max_queue_size: 4, max_events_per_10min: 6, ..test_config() };
    let telemetry = start(config, FakeTransport::scripted([]));
    tokio::time::sleep(Duration::from_millis(10)).await;
    for _ in 0..10 {
        telemetry.emit(TelemetryEvent::new("lk.ping"));
    }
    let stats = telemetry.stats();
    assert_eq!(stats.dropped_rate_limited, 4, "10 emitted, 6 admitted per 10 minutes");
    assert_eq!(stats.dropped_queue_full, 2, "6 admitted into a queue of 4");
    assert_eq!(stats.dropped, 6);
}

#[tokio::test(start_paused = true)]
async fn a_memory_cache_survives_failed_uploads_not_the_process() {
    let cache = Arc::new(MemoryCache::new(1 << 20));
    let (telemetry, exporter) = Telemetry::with_cache(
        test_config(),
        FakeTransport::scripted(offline_forever()),
        cache.clone(),
    );
    telemetry.override_endpoint("http://collector");
    tokio::spawn(exporter.run());
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    telemetry.shutdown().await;
    assert!(!cache.pending().is_empty(), "kept through the failure; gone with the process");
}

/// Opt-out while a request is in flight: whatever the collector answers afterwards, nothing is
/// written back, retried or resurrected. At most that one batch has left the device.
#[tokio::test(start_paused = true)]
async fn opting_out_during_an_upload_resurrects_nothing() {
    struct Gate(tokio::sync::Mutex<Option<tokio::sync::oneshot::Receiver<()>>>, Mutex<u32>);
    #[async_trait::async_trait]
    impl TelemetryTransport for Gate {
        async fn send(&self, _: ExportRequest) -> Result<ExportResponse, ExportError> {
            *self.1.lock().expect("lock") += 1;
            if let Some(gate) = self.0.lock().await.take() {
                let _ = gate.await;
                // The answer that arrives after the opt-out: "try again later".
                return Ok(ExportResponse { status: 503, ..Default::default() });
            }
            Ok(ExportResponse::accepted())
        }
    }
    let dir = temp_dir("optout-inflight");
    let (open, gate) = tokio::sync::oneshot::channel();
    let transport = Arc::new(Gate(tokio::sync::Mutex::new(Some(gate)), Mutex::new(0)));
    let (telemetry, _task) = killable(on_disk(&dir), transport.clone());
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    tokio::time::sleep(Duration::from_millis(1500)).await; // the upload is in flight
    assert_eq!(*transport.1.lock().expect("lock"), 1);
    let purge = telemetry.purge();
    let release = async {
        tokio::time::sleep(Duration::from_millis(5)).await;
        let _ = open.send(());
    };
    tokio::join!(purge, release);
    tokio::time::sleep(Duration::from_secs(600)).await;
    assert_eq!(*transport.1.lock().expect("lock"), 1, "no retry after the opt-out");
    assert_eq!(files_in(&dir), 0, "and nothing written back");
    assert!(telemetry.stats().dropped_purged >= 1, "the purge is counted locally");
    let _ = fs::remove_dir_all(&dir);
}

/// The wall clock jumps a day ahead: this launch's own batches are not expired by it (the
/// monotonic clock disagrees). A previous launch's genuinely old batch is:
/// `batches_older_than_a_day_are_dropped_and_counted_at_start`.
#[tokio::test(start_paused = true)]
async fn a_clock_jump_does_not_expire_a_fresh_backlog() {
    let transport = FakeTransport::scripted(offline_forever());
    let telemetry = start(test_config(), transport.clone());
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    telemetry.flush().await;
    crate::event::CLOCK_JUMP_NS.with(|jump| jump.set(25 * 3600 * 1_000_000_000));
    tokio::time::sleep(Duration::from_secs(120)).await;
    let stats = telemetry.stats();
    crate::event::CLOCK_JUMP_NS.with(|jump| jump.set(0));
    assert_eq!(stats.dropped_expired, 0, "the monotonic clock says minutes, not a day");
    assert!(stats.cached_batches >= 1);
}

/// The 24 h age limit applies while running, not only at start (an app left open offline): the
/// wall clock and the monotonic clock both say a day has passed.
#[tokio::test(start_paused = true)]
async fn batches_expire_while_the_app_runs() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start(test_config(), transport.clone());
    let offline = DeviceState { network: NetworkType::Unavailable, ..Default::default() };
    telemetry.set_device_state(offline);
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    telemetry.flush().await;
    let day = Duration::from_secs(24 * 3600 + 60);
    crate::event::CLOCK_JUMP_NS.with(|jump| jump.set(day.as_nanos() as i64));
    tokio::time::sleep(day).await;
    telemetry.flush().await;
    let stats = telemetry.stats();
    crate::event::CLOCK_JUMP_NS.with(|jump| jump.set(0));
    assert!(stats.dropped_expired >= 1, "expired without a restart");
    assert!(!transport.sent().iter().flat_map(event_names).any(|n| n == "lk.ping"));
}

/// The app's correlation attributes and custom events: Room-scoped, captured with each record,
/// per-event attributes win, the SDK's keys are off limits, over-long input is rejected and
/// counted — never truncated.
#[tokio::test(start_paused = true)]
async fn custom_data_is_room_scoped_validated_and_snapshotted() {
    use crate::Attribute;
    let transport = FakeTransport::scripted([]);
    let telemetry = start(test_config(), transport.clone());
    let (room, other) = (telemetry.begin_scope(), telemetry.begin_scope());
    room.set_attribute("app.call_id", Some("c1".into()));
    room.emit_custom("checkout", vec![Attribute::new("step", "pay")]);
    room.set_attribute("app.call_id", Some("c2".into())); // after capture: no rewrite
    room.emit_custom("override", vec![Attribute::new("app.call_id", "mine")]);
    other.emit_custom("elsewhere", vec![]);

    let long = "x".repeat(1025);
    room.set_attribute("lk.room.sid", Some("spoof".into()));
    room.set_attribute("session.id", Some("spoof".into()));
    room.set_attribute("app.long", Some(long.clone().into()));
    room.set_attribute(&"k".repeat(129), Some("v".into()));
    room.emit_custom(&"n".repeat(129), vec![]);
    room.emit_custom("bad", vec![Attribute::new("lk.track.sid", "spoof")]);
    room.emit_custom("bad", vec![Attribute::new("v", long.as_str())]);
    for n in 0..63 {
        room.set_attribute(&format!("app.k{n}"), Some("v".into()));
    }
    room.set_attribute("app.one_too_many", Some("v".into()));
    room.set_attribute("app.k0", Some("replaced".into())); // an existing key is not "one more"
    room.set_attribute("app.k1", None);
    assert_eq!(telemetry.stats().dropped_invalid, 8, "every rejection counted");

    telemetry.flush().await;
    let records: Vec<_> = transport.sent().iter().flat_map(records).collect();
    let find = |name: &str| records.iter().find(|r| r.event_name == name).expect(name).clone();
    let value = |r: &crate::proto::opentelemetry::proto::logs::v1::LogRecord, key: &str| {
        r.attributes
            .iter()
            .find(|a| a.key == key)
            .and_then(|a| a.value.clone())
            .map(|v| format!("{v:?}"))
    };
    assert!(value(&find("custom.checkout"), "app.call_id").is_some_and(|v| v.contains("c1")));
    assert!(value(&find("custom.checkout"), "step").is_some_and(|v| v.contains("pay")));
    assert!(value(&find("custom.override"), "app.call_id").is_some_and(|v| v.contains("mine")));
    assert_eq!(value(&find("custom.elsewhere"), "app.call_id"), None, "another room's own");
    assert!(!records.iter().any(|r| r.event_name == "custom.bad"));
}

/// The core sets the platform's `getStats()` cadence: fast only while a subscribe waits for its
/// first media, else two readings per (pressure-stretched) window.
#[tokio::test(start_paused = true)]
async fn the_core_paces_stats_polling() {
    use crate::{SpanTrack, ThermalState, TrackSource};
    let telemetry = start(TelemetryConfig::default(), FakeTransport::scripted([]));
    let room = telemetry.begin_scope();
    assert_eq!(room.stats_poll_interval_ms(), 30_000, "a 60 s window, polled twice");
    room.subscribe_started(SpanTrack {
        sid: Some("TR_1".into()),
        kind: TrackKind::Video,
        source: TrackSource::Camera,
        remote_identity: None,
    });
    assert_eq!(room.stats_poll_interval_ms(), 1_000, "first media is seen in the readings");
    room.track_ended("TR_1");
    telemetry
        .set_device_state(DeviceState { thermal: ThermalState::Serious, ..Default::default() });
    assert_eq!(room.stats_poll_interval_ms(), 60_000, "thermal pressure stretches it");
    // The fallback: a `subscribed` with no intent before it (a platform that missed a join-time
    // track's `subscribe_started`) is the intent, and polling turns fast at once.
    room.subscribed(SpanTrack {
        sid: Some("TR_join".into()),
        kind: TrackKind::Audio,
        source: TrackSource::Microphone,
        remote_identity: None,
    });
    assert_eq!(room.stats_poll_interval_ms(), 1_000, "a join-time track awaits first media");
}

/// A collector that takes `delay` to answer every request (then accepts it).
struct Slow {
    delay: Duration,
    sent: Mutex<u32>,
}

#[async_trait::async_trait]
impl TelemetryTransport for Slow {
    async fn send(&self, _: ExportRequest) -> Result<ExportResponse, ExportError> {
        *self.sent.lock().expect("lock") += 1;
        tokio::time::sleep(self.delay).await;
        Ok(ExportResponse::accepted())
    }
}

/// Finding r1-6: a request on the wire does not stall the actor — a subscribe deadline that
/// falls due while a 40 s upload is out is enforced on time.
#[tokio::test(start_paused = true)]
async fn deadlines_are_served_while_a_request_is_out() {
    use crate::{SpanTrack, TrackSource};
    let slow = Arc::new(Slow { delay: Duration::from_secs(40), sent: Mutex::new(0) });
    let config = TelemetryConfig { export_timeout_ms: 60_000, ..test_config() };
    let (telemetry, _task) = killable(config, slow.clone());
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    tokio::time::sleep(Duration::from_millis(1500)).await; // the tick put it on the wire
    assert_eq!(*slow.sent.lock().expect("lock"), 1);
    let room = telemetry.begin_scope();
    room.subscribe_started(SpanTrack {
        sid: Some("TR_1".into()),
        kind: TrackKind::Video,
        source: TrackSource::Camera,
        remote_identity: None,
    });
    assert_eq!(room.stats_poll_interval_ms(), 1_000, "awaiting first media");
    tokio::time::sleep(Duration::from_secs(31)).await; // the upload is still out
    assert_eq!(room.stats_poll_interval_ms(), 7_500, "timed out on time, not after the upload");
}

/// Finding r1-6: first media after the deadline is `timed_out` even when no sweep ran first.
#[tokio::test(start_paused = true)]
async fn late_first_media_is_timed_out_whoever_looks_first() {
    use crate::{SpanTrack, TrackSource};
    let transport = FakeTransport::scripted([]);
    let (telemetry, exporter) = Telemetry::new(test_config(), transport.clone());
    telemetry.override_endpoint("http://collector");
    let room = telemetry.begin_scope();
    room.subscribe_started(SpanTrack {
        sid: Some("TR_1".into()),
        kind: TrackKind::Audio,
        source: TrackSource::Microphone,
        remote_identity: None,
    });
    tokio::time::advance(crate::Scope::SUBSCRIBE_TIMEOUT + Duration::from_secs(1)).await;
    let mut media = RtcStatsSample::new("TR_1", TrackKind::Audio, StreamDirection::Inbound);
    media.bytes = Some(100);
    room.record_stats(media); // no exporter has swept yet
    tokio::spawn(exporter.run());
    telemetry.flush().await;
    let outcome = exported_spans(&transport)
        .into_iter()
        .flat_map(|s| s.attributes)
        .find(|a| a.key == "error.type")
        .and_then(|a| a.value)
        .map(|v| format!("{v:?}"));
    assert!(outcome.is_some_and(|o| o.contains("timed_out")), "not ok after the deadline");
}

/// Finding r1-6: shutdown is bounded and joins the exporter — a slow backlog is cut off at
/// `export_timeout_ms`, the request on the wire cancelled, the actor gone.
#[tokio::test(start_paused = true)]
async fn shutdown_during_a_slow_backlog_is_bounded_and_joins_the_exporter() {
    let slow = Arc::new(Slow { delay: Duration::from_secs(8), sent: Mutex::new(0) });
    let config = TelemetryConfig { max_batches_per_upload: 1, ..test_config() };
    let (telemetry, task) = killable(config, slow.clone());
    for _ in 0..10 {
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    let started = tokio::time::Instant::now();
    telemetry.shutdown().await;
    assert!(started.elapsed() <= Duration::from_millis(10_050), "{:?}", started.elapsed());
    tokio::task::yield_now().await;
    assert!(task.is_finished(), "the exporter stopped with shutdown");
    assert!(telemetry.stats().cached_batches > 0, "what did not go out stays cached");
}

/// Finding r1-14: a soft hold's one-minute cap is kept on its own clock, not at the next tick —
/// even when pressure stretched the cadence to four minutes — and a 413 split inside the escape
/// spends the escape's single request.
#[tokio::test(start_paused = true)]
async fn the_soft_hold_cap_is_scheduled_and_splits_count_against_it() {
    use crate::ThermalState;
    let transport =
        FakeTransport::scripted([Ok(ExportResponse { status: 413, ..Default::default() })]);
    let telemetry = start(TelemetryConfig::default(), transport.clone());
    tokio::time::sleep(Duration::from_millis(10)).await;
    telemetry.set_device_state(DeviceState {
        network_constrained: true,
        thermal: ThermalState::Serious,
        low_power_mode: Some(true),
        ..Default::default()
    });
    for _ in 0..4 {
        telemetry.emit(TelemetryEvent::new("lk.ping"));
    }
    telemetry.flush().await; // cached; the hold starts
    assert!(transport.sent().is_empty());
    tokio::time::sleep(Duration::from_secs(61)).await;
    assert_eq!(transport.sent().len(), 1, "one batch at the cap, not at the 240 s tick");
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(transport.sent().len(), 1, "the 413's halves wait for the next escape");
    tokio::time::sleep(Duration::from_secs(55)).await;
    assert_eq!(transport.sent().len(), 2, "the next escape, a minute later");
}

/// Finding r1-3: after the opt-out every capture path is closed — spans open detached,
/// subscribes do not start, stats and events go nowhere.
#[tokio::test(start_paused = true)]
async fn after_the_opt_out_nothing_is_captured() {
    use crate::{SpanTrack, TrackSource};
    let transport = FakeTransport::scripted([]);
    let telemetry = start(test_config(), transport.clone());
    let room = telemetry.begin_scope();
    telemetry.purge().await;
    let span = room.start(SpanName::Connect, None);
    assert!(span.context().is_none(), "detached: never registered");
    span.end(SpanOutcome::Ok, None);
    room.subscribe_started(SpanTrack {
        sid: Some("TR_1".into()),
        kind: TrackKind::Video,
        source: TrackSource::Camera,
        remote_identity: None,
    });
    assert_eq!(room.stats_poll_interval_ms(), 7_500, "no subscribe started");
    room.record_stats(RtcStatsSample::new("TR_1", TrackKind::Audio, StreamDirection::Inbound));
    room.emit(TelemetryEvent::new("lk.ping"));
    telemetry.set_device_state(DeviceState::default());
    assert_eq!(telemetry.shared.spans.lock().expect("spans").open_count(), 0);
    telemetry.flush().await;
    assert!(transport.sent().is_empty());
}

/// Finding r1-5: a pending subscribe does not keep the pipeline alive — once every handle is
/// dropped the exporter stops and the pipeline is freed, again and again.
#[tokio::test(start_paused = true)]
async fn a_pending_subscribe_never_keeps_the_pipeline_alive() {
    use crate::{SpanTrack, TrackSource};
    for _ in 0..3 {
        let (telemetry, exporter) = Telemetry::new(test_config(), FakeTransport::scripted([]));
        telemetry.override_endpoint("http://collector");
        let task = tokio::spawn(exporter.run());
        let weak = Arc::downgrade(&telemetry.shared);
        let room = telemetry.begin_scope();
        room.subscribe_started(SpanTrack {
            sid: Some("TR_1".into()),
            kind: TrackKind::Video,
            source: TrackSource::Camera,
            remote_identity: None,
        });
        drop(room);
        drop(telemetry);
        tokio::time::timeout(Duration::from_secs(30), task)
            .await
            .expect("the exporter stops once every handle is gone")
            .expect("exporter");
        assert!(weak.upgrade().is_none(), "nothing keeps the pipeline");
    }
}

fn chmod(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("chmod");
}

/// Finding r1-8: an accepted batch whose delete fails is not uploaded again; it is deleted as
/// soon as the storage allows.
#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn a_failed_delete_after_acceptance_is_not_sent_again() {
    let dir = temp_dir("undeletable");
    let transport = FakeTransport::scripted([crate::telemetry::tests::offline()]);
    let (telemetry, _task) = killable(on_disk(&dir), transport.clone());
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    telemetry.flush().await; // offline: on disk
    assert_eq!(files_in(&dir), 1);
    chmod(&dir, 0o555); // nothing can be deleted from here on
    tokio::time::sleep(Duration::from_secs(10)).await; // accepted, delete fails
    let sent = transport.sent().len();
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert_eq!(transport.sent().len(), sent, "not sent again every tick");
    chmod(&dir, 0o755);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(files_in(&dir), 0, "deleted once the storage allows");
    assert_eq!(transport.sent().len(), sent);
    let _ = fs::remove_dir_all(&dir);
}

/// Finding r1-8: a batch that exists but cannot be read right now (file protection on a locked
/// device, a permission) is not corrupt: it is kept and sent once readable.
#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn an_inaccessible_batch_is_kept_not_counted_corrupt() {
    let dir = temp_dir("inaccessible");
    let first = FakeTransport::scripted(offline_forever());
    let (telemetry, task) = killable(on_disk(&dir), first);
    telemetry.emit(TelemetryEvent::new("custom.locked"));
    telemetry.flush().await;
    task.abort();
    let _ = task.await;
    let file = fs::read_dir(&dir).expect("dir").flatten().next().expect("batch").path();
    chmod(&file, 0o000);
    let transport = FakeTransport::scripted([]);
    let (next, _task) = killable(on_disk(&dir), transport.clone());
    next.flush().await;
    assert!(transport.sent().is_empty());
    assert_eq!((next.stats().dropped_corrupt, files_in(&dir)), (0, 1), "kept, not corrupt");
    chmod(&file, 0o644);
    next.flush().await;
    let names: Vec<String> = transport.sent().iter().flat_map(event_names).collect();
    assert!(names.contains(&"custom.locked".to_owned()), "sent once readable");
    let _ = fs::remove_dir_all(&dir);
}

/// Finding r1-8: an opt-out that could not delete everything says so.
#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn an_incomplete_purge_is_reported() {
    let dir = temp_dir("purge-denied");
    let (telemetry, _task) = killable(on_disk(&dir), FakeTransport::scripted(offline_forever()));
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    telemetry.flush().await;
    chmod(&dir, 0o555);
    assert!(!telemetry.purge().await, "not everything could be deleted");
    chmod(&dir, 0o755);
    assert_eq!(files_in(&dir), 1, "the file is still there, as reported");
    let _ = fs::remove_dir_all(&dir);
}

/// A cache that writes to disk until told the disk is full: pushes and splits then fail.
struct FillingDisk {
    inner: crate::FileCache,
    full: std::sync::atomic::AtomicBool,
}

impl BatchCache for FillingDisk {
    fn push(&self, id: &str, body: &[u8]) -> io::Result<Vec<String>> {
        if self.full.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(io::Error::new(io::ErrorKind::StorageFull, "disk full"));
        }
        self.inner.push(id, body)
    }
    fn replace(&self, old: &str, new: &[(String, Vec<u8>)]) -> io::Result<Vec<String>> {
        if self.full.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(io::Error::new(io::ErrorKind::StorageFull, "disk full"));
        }
        self.inner.replace(old, new)
    }
    fn pending(&self) -> Vec<String> {
        self.inner.pending()
    }
    fn read(&self, id: &str) -> io::Result<Vec<u8>> {
        self.inner.read(id)
    }
    fn remove(&self, id: &str) -> io::Result<()> {
        self.inner.remove(id)
    }
    fn clear(&self) -> io::Result<()> {
        self.inner.clear()
    }
}

/// Finding r1-7: a 413 on a batch that lives on disk never moves its records to memory, where a
/// crash would lose them: when the disk cannot take the halves the batch stays whole, on disk.
#[tokio::test(start_paused = true)]
async fn a_split_never_moves_a_batch_off_the_disk() {
    let dir = temp_dir("split-full");
    let cache = Arc::new(FillingDisk {
        inner: crate::FileCache::open(&dir, 1 << 20).expect("open"),
        full: false.into(),
    });
    let too_large = || Ok(ExportResponse { status: 413, ..Default::default() });
    let transport = FakeTransport::scripted(
        std::iter::once(crate::telemetry::tests::offline())
            .chain(std::iter::repeat_with(too_large).take(64)),
    );
    let (telemetry, exporter) =
        Telemetry::with_cache(test_config(), transport.clone(), cache.clone());
    telemetry.override_endpoint("http://collector");
    tokio::spawn(exporter.run());
    for _ in 0..4 {
        telemetry.emit(TelemetryEvent::new("lk.ping"));
    }
    telemetry.flush().await; // offline: the batch of 4 is on disk
    assert_eq!(files_in(&dir), 1);
    cache.full.store(true, std::sync::atomic::Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(30)).await; // the retries answer 413
    assert!(transport.sent().len() >= 2, "the 413 came");
    assert_eq!(files_in(&dir), 1, "the batch is still on disk, whole");
    assert_eq!(telemetry.stats().dropped, 0);
    let _ = fs::remove_dir_all(&dir);
}

/// Finding r1-12: the request limit holds for the encoded body — session attributes added at
/// export, the self-report, span attributes included — not just for the pre-encoding estimate.
#[tokio::test(start_paused = true)]
async fn encoded_requests_stay_under_the_byte_limit_for_both_signals() {
    use crate::{AttributeValue, RoomIdentity};
    let limit = 4_096;
    let transport = FakeTransport::scripted([]);
    let config =
        TelemetryConfig { max_batch_bytes: limit, max_batches_per_upload: 100, ..test_config() };
    let telemetry = start(config, transport.clone());
    let room = telemetry.begin_scope();
    // Attached at export, invisible to the estimate that cuts batches: ~1 KB per record.
    room.set_room(RoomIdentity { name: Some("n".repeat(1_000)), ..Default::default() });
    for _ in 0..40 {
        room.emit(TelemetryEvent::new("lk.ping"));
    }
    for _ in 0..20 {
        let span = room.start(SpanName::Publish, None);
        span.set_attribute("detail".into(), AttributeValue::Str("d".repeat(900)));
        span.end(SpanOutcome::Ok, None);
    }
    telemetry.flush().await;
    let sent = transport.sent();
    let sizes: Vec<usize> =
        sent.iter().map(|r| crate::telemetry::tests::gunzip(&r.body).len()).collect();
    assert!(sizes.iter().all(|n| *n <= limit as usize), "{sizes:?}");
    assert_eq!(exported_spans(&transport).len(), 20, "nothing lost to the splitting");
    let logs: usize =
        sent.iter().filter(|r| r.url.ends_with("logs")).map(|r| records(r).len()).sum();
    assert_eq!(logs, 40);
}

/// Finding r1-15: RTC windows carry the correlation attributes they were captured under — a
/// change mid-window closes the window first, so no reading is filed under the wrong value.
#[tokio::test(start_paused = true)]
async fn rtc_windows_carry_the_attributes_of_their_readings() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start(test_config(), transport.clone());
    let room = telemetry.begin_scope();
    room.set_attribute("app.call_id", Some("before".into()));
    let reading = |bytes| RtcStatsSample {
        bytes: Some(bytes),
        ..RtcStatsSample::new("TR_1", TrackKind::Audio, StreamDirection::Inbound)
    };
    room.record_stats(reading(100));
    room.set_attribute("app.call_id", Some("after".into()));
    room.record_stats(reading(200));
    tokio::time::sleep(Duration::from_secs(16)).await; // the window closes
    telemetry.flush().await;
    let windows: Vec<(String, String)> = transport
        .sent()
        .iter()
        .flat_map(records)
        .filter(|r| r.event_name == "lk.rtc.stats.sample")
        .map(|r| {
            let get = |key: &str| {
                r.attributes
                    .iter()
                    .find(|a| a.key == key)
                    .and_then(|a| a.value.clone())
                    .map(|v| format!("{v:?}"))
                    .unwrap_or_default()
            };
            (get("app.call_id"), get("lk.rtc.bytes"))
        })
        .collect();
    assert_eq!(windows.len(), 2, "split at the change: {windows:?}");
    assert!(windows[0].0.contains("before") && windows[0].1.contains("100"), "{windows:?}");
    assert!(windows[1].0.contains("after") && windows[1].1.contains("200"), "{windows:?}");
}

/// Finding r2-5: the hold policy is evaluated before every request of a pass — the device going
/// offline while request 1 is out stops requests 2..n.
#[tokio::test(start_paused = true)]
async fn a_device_change_during_a_pass_stops_its_remaining_requests() {
    let slow = Arc::new(Slow { delay: Duration::from_secs(5), sent: Mutex::new(0) });
    let (telemetry, _task) = killable(test_config(), slow.clone());
    let offline = DeviceState { network: NetworkType::Unavailable, ..Default::default() };
    telemetry.set_device_state(offline);
    for _ in 0..3 {
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await; // offline: cached, nothing sent
    }
    assert_eq!(*slow.sent.lock().expect("lock"), 0);
    telemetry.set_device_state(DeviceState::default()); // online: a pass of 4 starts
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(*slow.sent.lock().expect("lock"), 1, "request 1 is out");
    telemetry.set_device_state(offline);
    tokio::time::sleep(Duration::from_secs(30)).await; // request 1 answers; the pass goes on
    assert_eq!(*slow.sent.lock().expect("lock"), 1, "offline: no request 2");
}

/// Finding r2-6: a soft hold that turns into a hard one does not leave a ready timer behind —
/// the exporter stays idle while offline past the soft cap, and the cap resumes on recovery.
#[tokio::test(start_paused = true)]
async fn a_soft_hold_turning_hard_does_not_spin() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start(TelemetryConfig::default(), transport.clone());
    tokio::time::sleep(Duration::from_millis(10)).await;
    telemetry.set_device_state(DeviceState { network_constrained: true, ..Default::default() });
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    telemetry.flush().await; // soft hold starts
    tokio::time::sleep(Duration::from_secs(50)).await;
    let offline = DeviceState {
        network: NetworkType::Unavailable,
        network_constrained: true,
        ..Default::default()
    };
    telemetry.set_device_state(offline);
    tokio::time::sleep(Duration::from_millis(10)).await;
    let before = telemetry.shared.passes.load(std::sync::atomic::Ordering::Relaxed);
    tokio::time::sleep(Duration::from_secs(150)).await; // far past the 60 s soft cap
    let passes = telemetry.shared.passes.load(std::sync::atomic::Ordering::Relaxed) - before;
    assert!(passes <= 4, "only the minute ticks wake it: {passes}");
    assert!(transport.sent().is_empty());
    telemetry.set_device_state(DeviceState { network_constrained: true, ..Default::default() });
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(transport.sent().len(), 1, "back online: the overdue soft cap lets one through");
}

/// Finding r2-9: the final request limits hold after everything is added — the self-report
/// takes one of `max_batch_size` places, a single record over `max_batch_bytes` is dropped and
/// counted, and over-long caller strings (attribute keys, span names, error types, identities)
/// are never retained.
#[tokio::test(start_paused = true)]
async fn final_limits_hold_after_decoration_and_caller_strings_are_bounded() {
    use crate::{AttributeValue, RoomIdentity};
    let transport = FakeTransport::scripted([crate::telemetry::tests::offline()]);
    let config = TelemetryConfig {
        max_batch_size: 5,
        max_batch_bytes: 2_048,
        max_batches_per_upload: 100,
        ..test_config()
    };
    let telemetry = start(config, transport.clone());
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    telemetry.flush().await; // offline: a failure to report
    for _ in 0..12 {
        telemetry.emit(TelemetryEvent::new("lk.ping"));
    }
    telemetry.emit(TelemetryEvent::new("custom.huge").with_body("x".repeat(4_000)));
    tokio::time::sleep(Duration::from_secs(2)).await;
    telemetry.flush().await;
    let sent = transport.sent();
    assert!(sent.iter().all(|r| records(r).len() <= 5), "the report fits within the count");
    assert!(sent.iter().all(|r| crate::telemetry::tests::gunzip(&r.body).len() <= 2_048));
    let names: Vec<String> = sent.iter().flat_map(event_names).collect();
    assert!(names.contains(&"lk.telemetry.report".to_owned()));
    assert!(!names.contains(&"custom.huge".to_owned()));
    assert_eq!(telemetry.stats().dropped_oversized, 1, "dropped and counted");

    let before = telemetry.stats().dropped_invalid;
    let room = telemetry.begin_scope();
    room.set_room(RoomIdentity { name: Some("n".repeat(2_000)), ..Default::default() });
    let named = room.start(SpanName::Custom { name: "s".repeat(200) }, None);
    assert!(named.context().is_none(), "an over-long span name is not recorded");
    assert_eq!(named.label(), "invalid", "nor retained");
    assert_eq!(
        crate::Span::detached(SpanName::Custom { name: "s".repeat(200) }).label(),
        "invalid"
    );
    let span = room.start(SpanName::Publish, None);
    span.set_attribute("k".repeat(200), AttributeValue::Str("v".into()));
    span.fail("E".repeat(500));
    telemetry.flush().await;
    let exported = exported_spans(&transport);
    let publish = exported.iter().find(|s| s.name == "lk.publish").expect("publish span");
    assert!(publish.attributes.iter().all(|a| a.key.len() <= 128));
    assert!(publish
        .attributes
        .iter()
        .any(|a| a.key == "error.type" && format!("{:?}", a.value).contains("invalid")));
    assert_eq!(
        telemetry.stats().dropped_invalid - before,
        4,
        "span name, identity, attribute key and error type: each counted"
    );
}

/// Finding r2-10: a window carries the owner it captured when it opened. Closed by the exporter
/// before an attribute change but queued after it, it still carries the old value.
#[tokio::test(start_paused = true)]
async fn a_window_closed_before_a_change_keeps_its_snapshot_whenever_it_is_queued() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start(test_config(), transport.clone());
    let room = telemetry.begin_scope();
    room.set_attribute("app.call_id", Some("before".into()));
    room.record_stats(RtcStatsSample::new("TR_1", TrackKind::Audio, StreamDirection::Inbound));
    let closed = telemetry.shared.windows.lock().expect("windows").close(); // the exporter's tick
    room.set_attribute("app.call_id", Some("after".into()));
    for window in closed {
        telemetry.shared.store.push(window); // enqueued after the change
    }
    telemetry.flush().await;
    let window = transport
        .sent()
        .iter()
        .flat_map(records)
        .find(|r| r.event_name == "lk.rtc.stats.sample")
        .expect("window");
    let value =
        window.attributes.iter().find(|a| a.key == "app.call_id").and_then(|a| a.value.clone());
    assert!(format!("{value:?}").contains("before"), "{value:?}");
}

/// Finding r2-10: a Room changing projects mid-window closes its windows first — the readings
/// before go to the first project, the readings after to the second.
#[tokio::test(start_paused = true)]
async fn a_project_change_splits_the_rtc_windows_by_project() {
    use crate::destination::tests::granted;
    let transport = FakeTransport::scripted([]);
    let telemetry = crate::telemetry::tests::start_cloud(test_config(), transport.clone());
    let room = telemetry.begin_scope();
    let reading = |bytes| RtcStatsSample {
        bytes: Some(bytes),
        ..RtcStatsSample::new("TR_1", TrackKind::Audio, StreamDirection::Inbound)
    };
    room.set_server("wss://a.livekit.cloud", &granted(3600));
    room.record_stats(reading(100));
    room.set_server("wss://b.livekit.cloud", &granted(3601));
    room.record_stats(reading(200));
    tokio::time::sleep(Duration::from_secs(16)).await;
    telemetry.flush().await;
    let windows: Vec<(String, String)> = transport
        .sent()
        .iter()
        .flat_map(|r| {
            let url = r.url.clone();
            records(r).into_iter().filter(|l| l.event_name == "lk.rtc.stats.sample").map(move |l| {
                let bytes = l
                    .attributes
                    .iter()
                    .find(|a| a.key == "lk.rtc.bytes")
                    .map(|a| format!("{:?}", a.value));
                (url.clone(), bytes.unwrap_or_default())
            })
        })
        .collect();
    assert_eq!(windows.len(), 2, "{windows:?}");
    assert!(windows.iter().any(|(url, b)| url.starts_with("https://a.") && b.contains("100")));
    assert!(windows.iter().any(|(url, b)| url.starts_with("https://b.") && b.contains("200")));
}

/// Finding r2-12: killed after the collector answered but before the batch was deleted, the
/// next launch sends it again — once. (Deletes that fail without a crash are the other source
/// of repeats: see `a_failed_delete_after_acceptance_is_not_sent_again`.)
#[tokio::test(start_paused = true)]
async fn a_crash_between_the_answer_and_the_delete_resends_the_batch_once() {
    use crate::cache::Step;
    let dir = temp_dir("ack-delete");
    let cache = Arc::new(crate::FileCache::open(&dir, 1 << 20).expect("open"));
    cache.inject(|step, _| {
        if step == Step::Delete {
            panic!("killed after the answer");
        }
        Ok(())
    });
    let first = FakeTransport::scripted([]);
    let (telemetry, exporter) = Telemetry::with_cache(test_config(), first.clone(), cache.clone());
    telemetry.override_endpoint("http://collector");
    let task = tokio::spawn(exporter.run());
    telemetry.emit(TelemetryEvent::new("custom.once"));
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(task.is_finished(), "the exporter was killed at the delete");
    assert_eq!(first.sent().len(), 1, "the collector answered");
    drop((telemetry, cache));

    let second = FakeTransport::scripted([]);
    let (next, _task) = killable(on_disk(&dir), second.clone());
    next.flush().await;
    let resent = second.sent().iter().flat_map(event_names).filter(|n| n == "custom.once").count();
    assert_eq!(resent, 1, "sent again, exactly once");
    let _ = fs::remove_dir_all(&dir);
}

/// Finding r2-8: a batch the disk cannot make durable is not claimed committed — it is kept in
/// memory (counted as a write error) and nothing of it stays on disk.
#[tokio::test(start_paused = true)]
async fn a_failed_directory_sync_keeps_the_batch_in_memory_not_half_on_disk() {
    use crate::cache::Step;
    let dir = temp_dir("sync-fail");
    let cache = Arc::new(crate::FileCache::open(&dir, 1 << 20).expect("open"));
    cache.inject(|step, _| match step {
        Step::SyncDir => Err(io::Error::other("fsync failed")),
        _ => Ok(()),
    });
    let transport = FakeTransport::scripted(offline_forever());
    let (telemetry, exporter) = Telemetry::with_cache(test_config(), transport, cache.clone());
    telemetry.override_endpoint("http://collector");
    tokio::spawn(exporter.run());
    telemetry.emit(TelemetryEvent::new("lk.ping"));
    telemetry.flush().await;
    assert_eq!(files_in(&dir), 0, "nothing of unknown durability on disk");
    let stats = telemetry.stats();
    assert_eq!((stats.cache_write_errors, stats.cached_batches, stats.dropped), (1, 1, 0));
    let _ = fs::remove_dir_all(&dir);
}

/// Finding r2-8: the opt-out counts what it really deleted, once — a second purge adds nothing,
/// and cached data it could not delete is not counted as purged.
#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn purges_count_only_actual_deletions_once() {
    let dir = temp_dir("purge-count");
    let (telemetry, _task) = killable(on_disk(&dir), FakeTransport::scripted(offline_forever()));
    for _ in 0..3 {
        telemetry.emit(TelemetryEvent::new("lk.ping"));
    }
    telemetry.flush().await; // 3 records on disk
    chmod(&dir, 0o300); // listing denied
    assert!(!telemetry.purge().await);
    assert_eq!(telemetry.stats().dropped_purged, 0, "nothing it could not delete is counted");
    chmod(&dir, 0o755);
    assert!(telemetry.purge().await);
    assert_eq!(telemetry.stats().dropped_purged, 3);
    assert!(telemetry.purge().await);
    assert_eq!(telemetry.stats().dropped_purged, 3, "counted once");
    let _ = fs::remove_dir_all(&dir);
}

/// Finding r2-3: the lifecycle is serialized — an opt-out racing an install waits for the
/// instrument's `start` to finish, then stops it: never stop-before-start.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_instrument_never_starts_after_the_opt_out_stopped_it() {
    use std::sync::mpsc;
    let _global = crate::global::TEST_LOCK.lock().await;
    crate::global::reset_for_test();
    struct Slow {
        log: Mutex<Vec<&'static str>>,
        entered: Mutex<Option<mpsc::Sender<()>>>,
    }
    impl crate::global::TelemetryInstrument for Slow {
        fn start(&self) {
            self.log.lock().expect("log").push("start begins");
            if let Some(entered) = self.entered.lock().expect("lock").take() {
                let _ = entered.send(());
            }
            std::thread::sleep(Duration::from_millis(200));
            self.log.lock().expect("log").push("start ends");
        }
        fn stop(&self) {
            self.log.lock().expect("log").push("stop");
        }
    }
    let (entered, started) = mpsc::channel();
    let instrument =
        Arc::new(Slow { log: Mutex::new(Vec::new()), entered: Mutex::new(Some(entered)) });
    let (telemetry, exporter) = Telemetry::new(test_config(), FakeTransport::scripted([]));
    tokio::spawn(exporter.run());
    let installing = {
        let instrument = instrument.clone();
        std::thread::spawn(move || crate::global::install(telemetry, vec![instrument]))
    };
    started.recv().expect("start entered");
    crate::global::disable().await; // races the running start()
    installing.join().expect("install");
    assert_eq!(*instrument.log.lock().expect("log"), ["start begins", "start ends", "stop"]);
    crate::global::reset_for_test();
}

/// Finding r2-2: the opt-out cancels and awaits every generation — a replaced pipeline still
/// draining has its request on the wire cancelled before `disable` returns, and sends nothing
/// afterwards.
#[tokio::test(start_paused = true)]
async fn the_opt_out_cancels_a_draining_generation_before_returning() {
    let _global = crate::global::TEST_LOCK.lock().await;
    crate::global::reset_for_test();
    struct Hang {
        sent: Mutex<u32>,
        cancelled: Arc<std::sync::atomic::AtomicBool>,
    }
    struct OnDrop(Arc<std::sync::atomic::AtomicBool>);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
    #[async_trait::async_trait]
    impl TelemetryTransport for Hang {
        async fn send(&self, _: ExportRequest) -> Result<ExportResponse, ExportError> {
            *self.sent.lock().expect("lock") += 1;
            let _cancelled = OnDrop(self.cancelled.clone());
            std::future::pending().await
        }
    }
    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let hang = Arc::new(Hang { sent: Mutex::new(0), cancelled: cancelled.clone() });
    let config = TelemetryConfig { export_timeout_ms: 600_000, ..test_config() };
    let (first, exporter) = Telemetry::new(config, hang.clone());
    first.override_endpoint("http://collector");
    tokio::spawn(exporter.run());
    crate::global::install(first, Vec::new());
    crate::global::emit(TelemetryEvent::new("lk.ping"));
    tokio::time::sleep(Duration::from_millis(1500)).await; // its request is on the wire
    let second = start(test_config(), FakeTransport::scripted([]));
    let replaced = crate::global::install(second, Vec::new()).expect("replaced");
    tokio::spawn(async move { replaced.shutdown().await }); // draining, as the SDKs do
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(!cancelled.load(std::sync::atomic::Ordering::SeqCst), "still out while draining");
    crate::global::disable().await;
    assert!(
        cancelled.load(std::sync::atomic::Ordering::SeqCst),
        "cancelled before disable returned"
    );
    tokio::time::sleep(Duration::from_secs(60)).await;
    assert_eq!(*hang.sent.lock().expect("lock"), 1, "nothing afterwards");
    crate::global::reset_for_test();
}

/// Android e2e flake (investigated): the device state an instrument pushes from `start`, right
/// after install and racing the exporter's first pass, always reaches the collector by the end
/// of a `flush` + `shutdown` — on a multi-threaded runtime, many times over.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_initial_device_state_always_ships_by_flush_and_shutdown() {
    use crate::global::TelemetryInstrument;
    let _global = crate::global::TEST_LOCK.lock().await;
    struct Device;
    impl TelemetryInstrument for Device {
        fn start(&self) {
            let state = DeviceState {
                thermal: crate::ThermalState::Nominal,
                low_power_mode: Some(false),
                ..Default::default()
            };
            crate::global::set_device_state(state);
        }
        fn stop(&self) {}
    }
    for run in 0..50 {
        crate::global::reset_for_test();
        let transport = FakeTransport::scripted([]);
        let (telemetry, exporter) = Telemetry::new(test_config(), transport.clone());
        telemetry.override_endpoint("http://collector");
        tokio::spawn(exporter.run());
        crate::global::install(telemetry, vec![Arc::new(Device)]);
        crate::global::emit(TelemetryEvent::new("lk.later"));
        crate::global::flush().await;
        crate::global::shutdown().await;
        let names: Vec<String> = transport.sent().iter().flat_map(event_names).collect();
        for name in ["thermal", "low_power", "memory", "network", "app_state"] {
            let name = format!("lk.device.{name}.changed");
            assert!(names.contains(&name), "run {run}: {name} missing from {names:?}");
        }
    }
    crate::global::reset_for_test();
}

/// Swift review: the opt-out is in effect when `disable()` returns — no scope, no install, no
/// capture — before its purge has run; the purge then deletes what was captured before.
#[tokio::test(start_paused = true)]
async fn the_opt_out_is_in_effect_before_its_purge_runs() {
    let _global = crate::global::TEST_LOCK.lock().await;
    crate::global::reset_for_test();
    let transport = FakeTransport::scripted([]);
    let installed = start(test_config(), transport.clone());
    crate::global::install(installed.clone(), Vec::new());
    let room = crate::global::scope().expect("scope");
    room.emit_custom("before", Vec::new());
    let purge = crate::global::disable(); // not awaited yet
    assert!(crate::global::scope().is_none() && crate::global::shared().is_none());
    assert!(installed.shared.revoked(), "every capture refuses from here on");
    room.emit_custom("after", Vec::new());
    let later = start(test_config(), transport.clone());
    assert!(crate::global::install(later, Vec::new()).is_some(), "a configure is refused");
    assert!(purge.await);
    installed.shutdown().await;
    assert!(transport.sent().is_empty(), "nothing captured before or after is sent");
    assert_eq!(installed.stats().dropped_purged, 1, "the record captured before is purged");
    crate::global::reset_for_test();
}

/// Findings r2-2 / r3 suggestion: the consent barrier on every capture path — a producer held
/// just before it commits (a log record, a span, an RTC reading), while the opt-out revokes and
/// clears, cannot leave anything behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_capture_racing_the_opt_out_leaves_nothing_behind() {
    use std::sync::mpsc;
    type Hook = Arc<dyn Fn() + Send + Sync>;
    for path in ["record", "span", "rtc"] {
        let (telemetry, _exporter) = Telemetry::new(test_config(), FakeTransport::scripted([]));
        let room = telemetry.begin_scope();
        let (held, holding) = mpsc::channel();
        let (release, released) = mpsc::channel::<()>();
        let (held, released) = (Mutex::new(held), Mutex::new(released));
        let hook: Hook = Arc::new(move || {
            let _ = held.lock().expect("held").send(());
            let _ = released.lock().expect("released").recv();
        });
        match path {
            "record" => *telemetry.shared.store.pause.lock().expect("pause") = Some(hook),
            _ => *telemetry.shared.pause.lock().expect("pause") = Some(hook),
        }
        let producer = {
            let (telemetry, room) = (telemetry.clone(), room.clone());
            std::thread::spawn(move || match path {
                "record" => telemetry.emit(TelemetryEvent::new("lk.ping")),
                "span" => drop(room.start(SpanName::Publish, None)),
                _ => room.record_stats(RtcStatsSample::new(
                    "TR_1",
                    TrackKind::Audio,
                    StreamDirection::Inbound,
                )),
            })
        };
        holding.recv().expect("the producer is about to commit");
        telemetry.shared.revoked.store(true, std::sync::atomic::Ordering::SeqCst);
        telemetry.shared.clear(); // the purge, while the producer is held
        release.send(()).expect("release");
        producer.join().expect("producer");
        assert!(telemetry.shared.store.is_empty(), "{path}: no record");
        assert_eq!(
            telemetry.shared.spans.lock().expect("spans").open_count(),
            0,
            "{path}: no span"
        );
        assert_eq!(
            telemetry.shared.windows.lock().expect("windows").tracked().0,
            0,
            "{path}: no window"
        );
    }
}

/// A newly published track keeps polling fast until its first outbound reading (bounded by
/// 30 s), so platforms need no post-publish nudge; `lk.connect` carries attempt 1 by default.
#[tokio::test(start_paused = true)]
async fn publishing_polls_fast_until_the_first_outbound_reading() {
    use crate::{SpanTrack, TrackSource};
    let transport = FakeTransport::scripted([]);
    let telemetry = start(TelemetryConfig::default(), transport.clone());
    let room = telemetry.begin_scope();
    let track = SpanTrack {
        sid: Some("TR_mic".into()),
        kind: TrackKind::Audio,
        source: TrackSource::Microphone,
        remote_identity: None,
    };
    let publish = room.start(SpanName::Publish, None);
    publish.set_track(track.clone());
    publish.end(SpanOutcome::Ok, None);
    assert_eq!(room.stats_poll_interval_ms(), 1_000, "awaiting the first outbound reading");
    let mut sent = RtcStatsSample::new("TR_mic", TrackKind::Audio, StreamDirection::Outbound);
    sent.bytes = Some(10);
    room.record_stats(sent);
    assert_eq!(room.stats_poll_interval_ms(), 30_000, "seen: back to the window cadence");

    let silent = room.start(SpanName::Publish, None);
    silent.set_track(SpanTrack { sid: Some("TR_cam".into()), ..track });
    assert_eq!(room.stats_poll_interval_ms(), 1_000);
    tokio::time::sleep(Duration::from_secs(31)).await;
    assert_eq!(
        room.stats_poll_interval_ms(),
        30_000,
        "a track that never sends stops the fast poll"
    );

    room.start(SpanName::Connect, None).end(SpanOutcome::Ok, None);
    telemetry.flush().await;
    let connect =
        exported_spans(&transport).into_iter().find(|s| s.name == "lk.connect").expect("connect");
    let attempt = connect
        .attributes
        .iter()
        .find(|a| a.key == "lk.connect.attempt")
        .map(|a| format!("{:?}", a.value));
    assert!(attempt.is_some_and(|a| a.contains("IntValue(1)")));
}

/// Finding r3-1: the conservative cadence holds under a storm of notifications — 30 subscribes
/// and 30 device-state calls (a hold lifting, holds toggling, thermal changes) within one flush
/// interval add at most one allowance of requests (`max_batches_per_upload`) and encode no new
/// batch; the next interval brings the next allowance.
#[tokio::test(start_paused = true)]
async fn notifications_within_an_interval_never_exceed_one_allowance() {
    use crate::{SpanTrack, ThermalState, TrackSource};
    let transport = FakeTransport::scripted([]);
    let telemetry = start(TelemetryConfig::default(), transport.clone());
    tokio::time::sleep(Duration::from_millis(10)).await; // the first tick is behind us
    let offline = DeviceState { network: NetworkType::Unavailable, ..Default::default() };
    telemetry.set_device_state(offline);
    for _ in 0..10 {
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await; // offline: cached, not sent
    }
    assert_eq!(telemetry.stats().cached_batches, 10);
    assert!(transport.sent().is_empty());

    let room = telemetry.begin_scope();
    room.set_server("wss://p.livekit.cloud", "token"); // in a call: uploads are metered
    telemetry.set_device_state(DeviceState::default()); // the hold lifts: one pass
    for n in 0..30 {
        room.subscribe_started(SpanTrack {
            sid: Some(format!("TR_{n}")),
            kind: TrackKind::Video,
            source: TrackSource::Camera,
            remote_identity: None,
        });
        let thermal = if n % 2 == 0 { ThermalState::Fair } else { ThermalState::Nominal };
        let constrained = n % 5 == 0;
        telemetry.set_device_state(DeviceState {
            thermal,
            network_constrained: constrained,
            ..Default::default()
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    telemetry.set_device_state(DeviceState::default());
    tokio::time::sleep(Duration::from_secs(40)).await; // still inside the first interval
    assert_eq!(transport.sent().len(), 4, "one allowance, however many wake-ups");
    assert_eq!(telemetry.stats().cached_batches, 6, "no batch encoded for a notification");
    tokio::time::sleep(Duration::from_secs(25)).await; // the next tick
                                                       // The tick encodes the interval's records behind the backlog; oldest first, one allowance.
    assert_eq!(transport.sent().len(), 8, "the next interval: exactly its own allowance");
}

/// A room in a call with `n` batches cached offline, then back online: one allowance goes out.
async fn in_call_with_backlog(n: usize) -> (Telemetry, crate::Scope, Arc<FakeTransport>) {
    let transport = FakeTransport::scripted([]);
    let telemetry = start(TelemetryConfig::default(), transport.clone());
    tokio::time::sleep(Duration::from_millis(10)).await; // the first tick is behind us
    let room = telemetry.begin_scope();
    room.set_server("wss://p.livekit.cloud", "token");
    telemetry
        .set_device_state(DeviceState { network: NetworkType::Unavailable, ..Default::default() });
    for _ in 0..n {
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await; // offline: cached, not sent
    }
    telemetry.set_device_state(DeviceState::default());
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(transport.sent().len(), 4, "one allowance");
    (telemetry, room, transport)
}

/// Finding r4-1: entering the background uploads everything at once even when the interval's
/// allowance is already spent — the app may be suspended any moment.
#[tokio::test(start_paused = true)]
async fn entering_the_background_flushes_even_with_the_allowance_spent() {
    let (telemetry, _room, transport) = in_call_with_backlog(6).await;
    telemetry.emit(TelemetryEvent::new("lk.last_words"));
    telemetry.set_device_state(DeviceState {
        app_state: crate::AppState::Background,
        ..Default::default()
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    let sent = transport.sent();
    assert_eq!(telemetry.stats().cached_batches, 0, "the whole cache went");
    assert!(event_names(sent.last().expect("sent")).contains(&"lk.last_words".to_owned()));
}

/// Finding r4-NB: the allowance meters uploads only next to a call. Once the room disconnects,
/// the next tick replays the whole backlog.
#[tokio::test(start_paused = true)]
async fn the_allowance_applies_only_while_a_room_is_in_a_call() {
    let (_telemetry, room, transport) = in_call_with_backlog(10).await;
    room.disconnected(crate::DisconnectReason::ClientInitiated);
    tokio::time::sleep(Duration::from_secs(61)).await; // the next tick
                                                       // 10 cached + the tick's own two batches: the room's disconnect record, the process's
                                                       // device records.
    assert_eq!(transport.sent().len(), 12, "no call: the backlog is not metered");
}

/// Finding r4-NB: pressure relief re-derives the pending tick at once — a period stretched to
/// 4 minutes by a critical thermal state shrinks back to one, and a tick already overdue runs.
#[tokio::test(start_paused = true)]
async fn relief_brings_the_pending_tick_forward() {
    let transport = FakeTransport::scripted([]);
    let telemetry = start(TelemetryConfig::default(), transport.clone());
    tokio::time::sleep(Duration::from_millis(10)).await; // the first tick is behind us
    let hot = DeviceState { thermal: crate::ThermalState::Critical, ..Default::default() };
    telemetry.set_device_state(hot);
    tokio::time::sleep(Duration::from_secs(90)).await;
    assert!(transport.sent().is_empty(), "stretched to 4 minutes");
    telemetry.set_device_state(DeviceState::default());
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(transport.sent().len(), 1, "the overdue tick runs at once");
}

/// Finding r4-NB: the halves of a split committed but not yet published are purged by the
/// opt-out and counted, like every other cached record.
#[tokio::test(start_paused = true)]
async fn an_opt_out_counts_the_halves_of_an_unpublished_split() {
    use crate::FileCache;
    let dir = temp_dir("purge-journal");
    let cache = FileCache::open(&dir, 1 << 20).expect("open");
    // A split past its commit point whose publish failed: the parent is gone, its halves are
    // only journaled (`<parent>@<half>.pend`), and nothing has listed the cache since.
    let stamp = crate::event::now_unix_nanos();
    let [parent, a, b] =
        ["000001-6", "000001a-3", "000001b-3"].map(|s| format!("{stamp:020}-{s}-l"));
    for half in [a, b] {
        fs::write(dir.join(format!("{parent}@{half}.pend")), b"half").expect("journal");
    }
    let (telemetry, exporter) =
        Telemetry::with_cache(test_config(), FakeTransport::scripted([]), Arc::new(cache));
    drop(exporter); // never ran: the journal is exactly as the split left it
    assert!(telemetry.purge().await);
    assert_eq!(telemetry.stats().dropped_purged, 6, "both halves, counted once");
    assert_eq!(files_in(&dir), 0);
    let _ = fs::remove_dir_all(&dir);
}

/// Codex final review B1: with `max_batch_size` 1, a self-report that is itself dropped
/// (oversized) or evicts another batch must not take the only place forever — every pass moves
/// real records and ends. (On the multi-threaded runtime a livelocked exporter shows as a flush
/// that never answers.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failing_self_report_never_starves_real_records() {
    for (config, cache) in [
        // Every record oversized: the report is dropped too.
        (TelemetryConfig { max_batch_size: 1, max_batch_bytes: 1, ..test_config() }, 1 << 20),
        // A cache of one batch: every push evicts the previous one.
        (TelemetryConfig { max_batch_size: 1, ..test_config() }, 1),
    ] {
        let transport = FakeTransport::scripted(offline_forever());
        let (telemetry, exporter) =
            Telemetry::with_cache(config, transport, Arc::new(MemoryCache::new(cache)));
        telemetry.override_endpoint("http://collector");
        tokio::spawn(exporter.run());
        for _ in 0..3 {
            telemetry.emit(TelemetryEvent::new("lk.ping"));
        }
        let flushed = tokio::time::timeout(Duration::from_secs(5), telemetry.flush()).await;
        assert!(flushed.is_ok(), "the pass ends (cache {cache})");
        let stats = telemetry.stats();
        assert!(stats.dropped_oversized + stats.dropped_cache_full >= 2, "{stats:?}");
        assert!(telemetry.shared.store.is_empty(), "every real record moved");
    }
}
