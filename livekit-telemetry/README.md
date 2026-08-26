# LiveKit Telemetry

**Important**:
This is an internal crate that powers client telemetry in LiveKit client SDKs (including [Rust](https://crates.io/crates/livekit) and, through `livekit-uniffi`, Swift/Kotlin/Dart) and is not usable directly.

The core buffers events on-device and ships them out-of-band as standard
[OTLP/HTTP](https://opentelemetry.io/docs/specs/otlp/) log records. Everything hard lives here
once — batching, encoding, retry, persistence, go-silent — while platforms provide only what
they are uniquely placed to: instruments (OS signals) and the byte-moving transport.

```text
 SDK / instruments ──emit()──▶ Telemetry ──▶ Store ──drain──▶ Exporter ──push──▶ BatchCache ──upload oldest-first──▶ TelemetryTransport
                              (source)      (events)          (tick · encode)   (MemoryCache | FileCache | yours)   (NetTransport | host HTTP | data channel …)
```

| Layer | OTel equivalent | Type |
|---|---|---|
| source | `Logger.emit` | [`Telemetry::emit`] with [`TelemetryEvent`] |
| store | `BatchLogRecordProcessor` queue | `Store` (in-memory events, drop-oldest) |
| sink | `BatchLogRecordProcessor` timer + `LogRecordExporter` | [`Exporter`] (actor, spawn `run()`) |
| cache | disk-buffering exporter wrapper | [`BatchCache`] trait: [`MemoryCache`] (default), [`FileCache`] (`storage_dir`), or your own via [`Telemetry::with_cache`] |
| transport | exporter's HTTP client | [`TelemetryTransport`] trait; `NetTransport` over `livekit-net` (feature `net`) |

## Usage

```rust,ignore
let mut config = TelemetryConfig::new("http://localhost:4318/v1/logs");
config.resource.push(Attribute::new("service.name", "my-app"));
config.storage_dir = Some(cache_dir.join("livekit-telemetry").display().to_string());
let transport = NetTransport::from_registry().expect("livekit-net client");

let (telemetry, exporter) = Telemetry::new(config, Arc::new(transport));
tokio::spawn(exporter.run());

telemetry.emit(TelemetryEvent::new("lk.ping").with_attribute("lk.ping.seq", 1i64));
telemetry.emit(TelemetryEvent::new("").with_severity(Severity::Error).with_body("connect failed")); // log record
let mut sample = RtcStatsSample::new("TR_abc", TrackKind::Audio, StreamDirection::Inbound);
sample.bytes = Some(48_000);
sample.jitter_ms = Some(12.0);
telemetry.record_stats(sample); // every 1–2 s; shipped as one lk.rtc.stats.sample per 15 s window
telemetry.set_device_state(DeviceState { thermal: ThermalState::Serious, ..Default::default() });
telemetry.shutdown().await; // cache, then upload what the network allows; bounded by `export_timeout_ms`
println!("{:?}", telemetry.stats()); // drops by reason, uploads, cached batches
```

Event names and attributes are defined in [`SPEC.md`](SPEC.md).

## Local testing

```sh
docker run -d --name lk-lgtm -p 3000:3000 -p 4318:4318 -p 3100:3100 grafana/otel-lgtm
cargo run -p telemetry_ping                          # sends one `lk.ping`
LK_TELEMETRY_DIR=/tmp/lk-telemetry cargo run -p telemetry_ping   # with the file cache
open http://localhost:3000/explore                   # Loki: {service_name="telemetry_ping"}
```

`LK_OTLP_ENDPOINT` overrides the collector URL. Stop the container, run with `LK_TELEMETRY_DIR`,
start it again and run once more to watch the cached batch replay.

## Design notes

- **Typed events in the store, OTLP at the edge.** Events are stored as [`TelemetryEvent`], not
  pre-serialized bytes: batches need grouping under one resource/scope, batch-time attributes
  can be attached, and a human-readable dump is a `Debug`/JSON view away.
- **Write-ahead cache between exporter and transport.** Every batch is encoded and pushed to
  the [`BatchCache`] *before* the network is tried, then uploaded oldest-first and removed on
  success — the shape every client SDK converges on (Sentry envelopes, Datadog batch files,
  opentelemetry-android disk buffering, Amplitude/Segment event files). The exporter only knows
  the trait: [`MemoryCache`] by default (failed uploads wait for the next attempt, lost with the
  process), [`FileCache`] with `storage_dir` (survives crashes; the next launch replays), or a
  caller-provided implementation. Cached bodies are ready-to-send OTLP with URL/headers
  recomposed at upload time, so a rotated token just works. A failed upload pauses the cache
  for a minute; a `Retry-After` keeps cached batches but drops new ones for its duration
  (throttling must not become a disk-backed queue); `Disabled` empties the cache. `FileCache`
  writes `.tmp` → rename, evicts oldest above `max_cache_bytes`, expires after 24 h using the
  timestamp in the file name (not file metadata — an Apple required-reason API), and treats a
  full disk as a counted drop with a single warning.
- **Logs are records, events have names.** A `TelemetryEvent` with an empty `name` is a plain
  OTLP log record (`severity` + `body`); only `Warn`/`Error` leave the device, `emit` drops the
  rest. Discrete events are capped at `max_events_per_10min` (300, the design doc's flood
  guard); RTC windows and self-telemetry are exempt.
- **RTC stats are windowed on device.** Platforms push raw `getStats()` readings as
  [`RtcStatsSample`]s every 1–2 s; the core ships one `lk.rtc.stats.sample` per track and
  direction per `stats_window_ms` (15 s, stretched with the cadence) — cumulative counters as
  the last value (monotonic, W3C webrtc-stats model), gauges as min/max/avg. Windows close early
  on background and shutdown.
- **Self-telemetry rides along.** Counters for every way data can be lost (`queue_full`,
  `cache_error`, `rejected`, `throttled`, `disabled`) and for uploads (`sent`, `failures`) are
  readable via [`Telemetry::stats`] and shipped as an `lk.telemetry.report` event appended to
  the next batch whenever something went wrong since the previous report — the Sentry
  "client report" shape, with reason names from the OTel SDK self-metrics conventions. Never
  an extra request, never persisted on its own, silent when nothing is wrong.
- **Device state comes from the host; the policy lives here.** Thermal, low-power and
  foreground/background are OS APIs the host already watches; it pushes them through
  [`Telemetry::set_device_state`]. The core emits the `lk.device.*.changed` events from
  `SPEC.md` and stretches its cadence up to 4× under pressure (background also flushes once).
  No Rust crate can observe these without a JVM/ObjC bridge, and `device-info` (in this
  workspace) covers static facts, not state.
- **Transport is the only injection point.** The core composes URL, headers and body; the
  transport moves bytes and reports [`ExportError`] so the core alone decides retry / drop /
  persist / go-silent. No Rust HTTP/TLS stack is linked unless the `net` feature is enabled.
- **Size.** OTLP types come from `opentelemetry-proto` (`gen-tonic-messages`, no tonic). Its
  `opentelemetry`/`opentelemetry_sdk` dependencies are dead code here and LTO removes them
  (measured +8 B on the iOS UniFFI dylib versus vendored generated code) — provided the
  workspace links a single `prost`, which is why `livekit-protocol` and friends moved to the
  workspace `prost` version.
