---
webrtc-sys: patch
---

An empty data channel message no longer aborts a debug build: libwebrtc passes it with a null pointer, which `slice::from_raw_parts` refuses, so a zero-length message is now an empty slice.
