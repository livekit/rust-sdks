# LiveKit Telemetry

**Important**:
This is an internal crate that powers client telemetry in LiveKit client SDKs (including [Rust](https://crates.io/crates/livekit) and, through `livekit-uniffi`, Swift/Kotlin/Dart) and is not usable directly.

The core buffers events on-device and ships them out-of-band as standard
[OTLP/HTTP](https://opentelemetry.io/docs/specs/otlp/) log records. Everything hard lives here
once — batching, encoding, retry, persistence, go-silent — while platforms provide only what
they are uniquely placed to: instruments (OS signals) and the byte-moving transport.

```text
 SDK / instruments ──emit()──▶ Telemetry ──▶ Store ──drain──▶ Exporter ──ExportRequest──▶ TelemetryTransport
                              (source)      (bounded queue)   (tick · OTLP encode · retry)    (NetTransport | host HTTP | data channel …)
                                                                   │ failed after retries
                                                                   ▼
                                                              FileCache (one file per encoded batch, replayed oldest-first)
```

| Layer | OTel equivalent | Type |
|---|---|---|
| source | `Logger.emit` | [`Telemetry::emit`] with [`TelemetryEvent`] |
| store | `BatchLogRecordProcessor` queue | `Store` (in-memory, drop-oldest) |
| sink | `BatchLogRecordProcessor` timer + `LogRecordExporter` | [`Exporter`] (actor, spawn `run()`) |
| transport | exporter's HTTP client | [`TelemetryTransport`] trait; `NetTransport` over `livekit-net` (feature `net`) |
| persistence | disk-buffering exporter wrapper | `FileCache` (opt-in via `storage_dir`) |

## Usage

```rust,ignore
let mut config = TelemetryConfig::new("http://localhost:4318/v1/logs");
config.resource.push(Attribute::new("service.name", "my-app"));
config.storage_dir = Some(cache_dir.join("livekit-telemetry").display().to_string());
let transport = NetTransport::from_registry().expect("livekit-net client");

let (telemetry, exporter) = Telemetry::new(config, Arc::new(transport));
tokio::spawn(exporter.run());

telemetry.emit(TelemetryEvent::new("lk.ping").with_attribute("lk.ping.seq", 1i64));
telemetry.shutdown().await; // spill to disk, then flush; bounded by `export_timeout_ms`
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
- **Persistence is a file cache at the exporter, not a database.** What every client SDK does:
  Sentry (envelope files, capped count, oldest evicted), Datadog (batch files, 5 MB/file,
  18 h max age), opentelemetry-android disk-buffering (`.tmp` → rename, 1 MB files, 10 MB dir),
  Amplitude/Segment (delimited JSON files, ~1 MB / 475 KB rotation). We persist the *encoded*
  request body: nothing to re-encode on replay, URL/headers recomposed from the current config
  (rotated tokens just work), zero dependencies. Throttled (`Retry-After`), rejected and
  disabled data is never written (persistence must not become a disk-backed retry queue).
  `shutdown` spills the queue to disk before trying the network, so an app killed offline
  loses nothing. Age is derived from file names, not file timestamps (an Apple
  required-reason API).
- **Transport is the only injection point.** The core composes URL, headers and body; the
  transport moves bytes and reports [`ExportError`] so the core alone decides retry / drop /
  persist / go-silent. No Rust HTTP/TLS stack is linked unless the `net` feature is enabled.
- **Size.** OTLP types come from `opentelemetry-proto` (`gen-tonic-messages`, no tonic). Its
  `opentelemetry`/`opentelemetry_sdk` dependencies are dead code here and LTO removes them
  (measured +8 B on the iOS UniFFI dylib versus vendored generated code) — provided the
  workspace links a single `prost`, which is why `livekit-protocol` and friends moved to the
  workspace `prost` version.
