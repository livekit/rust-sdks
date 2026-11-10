---
livekit-ffi: minor
livekit-telemetry: patch
---

Export the telemetry `ReconnectReason` to Swift, Kotlin and Python as `TelemetryReconnectReason`, so it no longer collides with the signaling `ReconnectReason`. The Rust name is unchanged.
