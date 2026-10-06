---
livekit-uniffi: minor
---

Expose the client telemetry core over UniFFI: `telemetry_configure` / `telemetry_configure_pulled` (a bounded pull queue for bindings with thread-bound callbacks, served with `next` or polled with `try_next`), `telemetry_scope` with `TelemetryScope.set_server` (connect and every token refresh), spans, the subscribe lifecycle, `record_peer_stats` + `stats_poll_interval_ms`, `track_ended`, `emit_custom` / `set_attribute` (the Room-scoped app API), `telemetry_disable` (a synchronous opt-out, in effect when it returns), device state and log forwarding, with a host-implemented `TelemetryTransport` or the `livekit-net` HTTP client.
