---
livekit-ffi: minor
---

Expose the client telemetry core through the `core-modules` UniFFI surface (`telemetry_configure`, `telemetry_configure_pulled`, `telemetry_scope`, spans, stats and `telemetry_disable`); `log_forward_bootstrap` now copies the core's own warnings and errors into telemetry and keeps the Rust log level at `warn` or looser.
