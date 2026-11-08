---
libwebrtc: patch
livekit-ffi: patch
---

Aggregate explicitly requested unbounded audio receive frames before waking the Rust consumer, preserving PCM, filtered streams, and bounded queue behavior.
