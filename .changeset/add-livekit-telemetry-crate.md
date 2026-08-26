---
livekit-telemetry: minor
---

Add `livekit-telemetry`, the shared client telemetry core: events are buffered on-device,
batched and exported as OTLP/HTTP log records through a pluggable `TelemetryTransport`. Batches are
written to a `BatchCache` (in memory by default, on disk with `storage_dir`) before upload, so
failed uploads, crashes and offline shutdowns lose nothing.
