---
livekit-ffi: minor
livekit-telemetry: patch
---

Export the telemetry `ReconnectReason` to Swift, Kotlin, Python and Dart as `TelemetryReconnectReason`, so it no longer collides with the signaling `ReconnectReason`. Foreign code that names the telemetry enum must rename `ReconnectReason` to `TelemetryReconnectReason`. The Rust name, the variants and `telemetry_reconnect_reason` are unchanged.
