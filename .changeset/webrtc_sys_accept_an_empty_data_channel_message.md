---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

An empty data channel message no longer aborts a debug build: libwebrtc passes it with a null pointer, which `slice::from_raw_parts` refuses, so a zero-length message is now an empty slice.
