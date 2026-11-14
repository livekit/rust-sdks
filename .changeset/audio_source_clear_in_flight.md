---
libwebrtc: patch
livekit-ffi: patch
webrtc-sys: patch
---

`clear_buffer` stops captures in flight: a frame longer than the queue no longer keeps playing after a clear, and a capture right after a clear is no longer refused.
