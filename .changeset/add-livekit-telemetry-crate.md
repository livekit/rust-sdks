---
livekit-telemetry: minor
---

Add `livekit-telemetry`, the shared client telemetry core: events are buffered on-device,
batched and exported as OTLP/HTTP log records through a pluggable `TelemetryTransport`. Batches are
written to a `BatchCache` (in memory by default, on disk with `storage_dir`) before upload, so
failed uploads, crashes and offline shutdowns lose nothing. Pipeline health is exposed as `Telemetry::stats` and shipped as `lk.telemetry.report`
events; hosts push `DeviceState` (thermal, low power, foreground/background) and the core emits
the `lk.device.*.changed` events and stretches its cadence under pressure. Log records (`Warn`/`Error` only), a flood guard for discrete events, on-device RTC
stats windows (`record_stats` → `lk.rtc.stats.sample`) and session-wide attributes complete the
design doc's v0 surface. Spans (`begin_span`/`add_span_event`/`end_span`, one attempt per span, the session as the
trace) ship on the traces signal through the same cache; every record carries the session trace id.
