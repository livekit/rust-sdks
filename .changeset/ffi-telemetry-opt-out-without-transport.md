---
livekit-ffi: patch
---

A `telemetry_configure` call with no transport after the opt-out is now refused like any other configure (`Ok`, storage erased) instead of failing with `NoTransport`, and the Android build pins `cargo-ndk` to 4.1.2.
