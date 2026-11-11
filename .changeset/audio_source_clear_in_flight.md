---
libwebrtc: patch
livekit-ffi: patch
---

`clear_buffer` also stops captures in flight: a frame longer than the queue no longer refills it after a clear.
