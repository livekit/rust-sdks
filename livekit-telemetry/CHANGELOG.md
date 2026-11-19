# Changelog
## 0.3.0 (2026-11-18)

### Breaking Changes

- Take every telemetry name from the protocol registry (`livekit_protocol::semconv`) and send its spelling: `lk.room_id`, `lk.participant_id`, `lk.track_id`, `lk.pii.room_name`, `lk.pii.participant_identity`, `lk.pii.remote_participant_identity`, `lk.track_source` with the RTC FFI `TrackSource` names, and `lk.disconnect_reason` with the protobuf `DisconnectReason` names.

### Features

- Rename the exported RTC sample field and stats report argument from `track_sid` to `track_id`; Rust and foreign callers must migrate the input name while continuing to pass the same LiveKit track identifier values.
- Limit the FFI lifecycle to three admitted telemetry generations and return AtCapacity before constructing a rejected replacement; pulled configuration now returns a Result and callers must keep serving previous queues when replacement fails.
- Add `global::log_with`, which builds a log record only while a pipeline is installed, and free the instruments that `global::uninstall` and `global::disable` remove outside the installed-pipeline lock, so an instrument destructor can log.
- Test the telemetry pipeline end to end against the backend contract: routing, tokens, collector answers, retries, holds and the write-ahead cache.
- Add the telemetry pipeline: per-room scopes, typed spans, and an exporter that caches and uploads OTLP batches under a device-aware cadence and hold policy.
- Add `global`, a process-wide telemetry pipeline with platform instruments and a synchronous opt-out that retries incomplete erasures.
- Convert a `log::Level` into a telemetry `Severity` with `From`, and remove the process pipeline without draining it with `global::uninstall`.
- Clarify exclusive ownership of the configured telemetry cache directory and the conservative opt-out result when a final per-cache clear fails.
- Recover cached batches with streaming splits above an 8 MiB threshold for decoded input. Retain unsupported batches and remove stale metadata after parent eviction.
- Rename telemetry stats report input arguments to `track_id` alongside the breaking `RtcStatsSample` field migration while preserving track values and existing report admission semantics.

### Fixes

- Clearing the telemetry disk cache now syncs the directory once for its deletion sweep instead of once per batch, and still syncs it when a deletion fails.
- Clearing the telemetry disk cache, as the opt-out does, now also deletes stray files in the storage directory and reports an entry it cannot delete, so the opt-out no longer reports success while a stray file stays.
- A Room that connects while the exporter retires unused credentials keeps its token, so its telemetry no longer waits forever without uploading.
- Export the telemetry `ReconnectReason` to Swift, Kotlin, Python and Dart as `TelemetryReconnectReason`, so it no longer collides with the signaling `ReconnectReason`. Foreign code that names the telemetry enum must rename `ReconnectReason` to `TelemetryReconnectReason`. The Rust name, the variants and `telemetry_reconnect_reason` are unchanged.
- A telemetry pipeline that `install` refuses after the opt-out is now cancelled, so its exporter stops by itself (the caller still joins the task), and it stays known to later `disable` calls, which retry a failed delete of its caller-provided cache; installing the installed pipeline again no longer hands it back as the outgoing one.
- Clarify that a failed batch split can report an already evicted parent without resurrecting records or counting the loss twice.
- Keep telemetry correlation consistent: a room identity change closes the open RTC windows and applies all four fields together, scoped records and spans no longer link to a span of another session, and `max_cache_bytes` gets its documented one-byte floor.
- A telemetry shutdown now gives the records and spans that a retained scope captures during its final drain further bounded encoding turns, which start only before the original export deadline; a started turn may finish after that deadline, and what is still queued when no further turn may start is not cached.

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
