---
livekit-capture: minor
livekit-ffi: minor
---

Add a `source-gstreamer-dynamic` feature that loads the GStreamer capture source at runtime from the `livekit-capture-gstreamer` shim library, use it for the FFI `capture-gstreamer` feature, and move the `livekit` dependency of `livekit-capture` behind a new default `rtc` feature.
