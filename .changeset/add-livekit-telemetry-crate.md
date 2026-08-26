---
livekit-telemetry: minor
---

Add `livekit-telemetry`, the shared client telemetry core: events are buffered on-device,
batched and exported as OTLP/HTTP log records through a pluggable `TelemetryTransport`, with an
optional on-disk cache of undeliverable batches.
