# Changelog
## 0.2.0 (2026-11-13)

### Breaking Changes

- Add an opt-in decoded HTTP response limit, used by telemetry export, that reports an oversized response as the new `TransportError::ResponseTooLarge` variant. Code that matches `TransportError` exhaustively, in Rust or in the generated Swift and Kotlin bindings, needs a case for it.

### Features

- Add the write-ahead batch cache (`BatchCache`, `MemoryCache`, `FileCache`) that holds encoded batches in memory or on disk until they upload.
- Add device state and device events (`DeviceState`, `DeviceEvent`) and RTC stats samples (`RtcStatsSample`, `RtcStat`) with their on-device windowing.
- Bound telemetry collector responses to 65,536 decoded bytes and retain batches with Retry-After on overflow. Document in `SPEC.md` that the exporter, not the transport, enforces the per-attempt export timeout and the 24-hour cap on server-directed pauses.
- Rename the public `RtcStatsSample.track_sid` field and constructor parameter to `track_id`; callers and generated bindings must migrate to the new name while keeping the same LiveKit track identifier values.
- Add the `TelemetryTransport` seam, a `net` HTTP transport over `livekit-net`, and LiveKit Cloud destination routing from a Room's server URL and token.

### Fixes

- Serialize cache writes and clears across process instances and reject replacements whose parent is already missing without resurrecting evicted batches.
- Compact unowned recomputable dead destinations while preserving all-dead routing, disabled projects and remembered consent.
- Speed up deduplication of large telemetry attribute lists while preserving last-value precedence and attribute order.

## 0.1.1 (2026-10-20)

### Features

- Add `livekit-telemetry`, the internal client telemetry core shared by the LiveKit client SDKs.
