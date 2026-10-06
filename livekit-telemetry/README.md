# LiveKit Telemetry

**Important**:
This is an internal crate that powers client telemetry in LiveKit client SDKs (through
`livekit-uniffi`: Swift, Kotlin, Dart) and is not meant to be used directly.

The core buffers records on the device and ships them out-of-band as standard
[OTLP/HTTP](https://opentelemetry.io/docs/specs/otlp/) logs and traces to the LiveKit Cloud
project a room belongs to. Everything hard lives here once — destination and credentials,
batching, encoding, retry, persistence, holds, loss accounting — while platforms provide only
what they are uniquely placed to: OS signals (instruments) and the byte-moving transport.

```text
 SDK / instruments ──emit()──▶ Telemetry ──▶ Store ──drain──▶ Exporter ──push──▶ BatchCache ──upload oldest-first──▶ TelemetryTransport
                              (source)      (records)         (tick · encode)   (MemoryCache | FileCache)            (NetTransport | host HTTP)
```

| Layer | OTel equivalent | Type |
|---|---|---|
| source | `Logger.emit`, `Tracer.start` | [`Telemetry`], [`Scope`] (one per room), [`Span`] |
| store | `BatchLogRecordProcessor` queue | `Store` (in-memory, drop-oldest, bounded) |
| exporter | batch processor timer + exporter | [`Exporter`] (actor, spawn `run()`) |
| cache | disk buffering | [`BatchCache`]: [`MemoryCache`], [`FileCache`] (`storage_dir`) |
| transport | exporter's HTTP client | [`TelemetryTransport`]; `NetTransport` over `livekit-net` (feature `net`) |

## Usage

```rust
# use std::sync::Arc;
# use livekit_telemetry::*;
# struct Discard;
# #[async_trait::async_trait]
# impl TelemetryTransport for Discard {
#     async fn send(&self, _: ExportRequest) -> Result<ExportResponse, ExportError> {
#         Ok(ExportResponse::accepted())
#     }
# }
# #[tokio::main(flavor = "current_thread")] async fn main() {
let config = TelemetryConfig { storage_dir: None, ..Default::default() };
let (telemetry, exporter) = Telemetry::new(config, Arc::new(Discard));
tokio::spawn(exporter.run());

// One scope per room. Its server URL and token decide where its records go; hand the token
// over again on every refresh.
let room = telemetry.begin_scope();
room.set_server("wss://my-project.livekit.cloud", "<participant token>");
room.set_attribute("app.call_id", Some("c-42".into()));
room.emit_custom("checkout", vec![Attribute::new("plan", "pro")]);

let connect = room.start(SpanName::Connect, None);
connect.step(SpanStep::WsOpen);
connect.end(SpanOutcome::Ok, None);

telemetry.set_device_state(DeviceState { thermal: ThermalState::Serious, ..Default::default() });
telemetry.shutdown().await; // cache, then upload what the network allows
println!("{}", telemetry.stats()); // drops by reason, uploads, cached batches
# }
```

Event names, attributes, cadences and the upload policy are defined in [`SPEC.md`](SPEC.md).

## Design notes

- **The room decides the destination.** A platform passes only the server URL and the token
  ([`Scope::set_server`]). The core derives the ingest URL (LiveKit Cloud hosts only), reads
  the token's unverified claims (observability grant, expiry) and routes each batch by its
  owner — project and session — so two rooms on two projects never share a token.
- **Write-ahead cache.** Every batch is encoded, gzipped and cached *before* the network is
  tried, then uploaded oldest-first and removed on an answer. Tokens are attached at upload
  time and never persisted.
- **The core reads every answer.** Transports return status, headers and body; the core alone
  classifies OTLP partial success, 4xx, 413 (split), 429/503 `Retry-After` and `RetryInfo`,
  and Cloud's disabled and credential answers.
- **Loss is counted, never silent.** Every way a record can be lost has a counter; the deltas
  ride along with the next batch as `lk.telemetry.report` and are readable locally through
  [`Telemetry::stats`].
- **Device state comes from the host; the policy lives here.** Thermal, power, memory,
  network and lifecycle are OS APIs the host watches and pushes through
  [`Telemetry::set_device_state`]; the core stretches its cadence and holds uploads.
- **Size.** OTLP types come from `opentelemetry-proto` (`gen-tonic-messages`, no tonic); JWT
  claims are read with `base64` and `serde_json`, both already linked by the UniFFI library.
