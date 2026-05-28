---
webrtc-sys: patch
---

Fix Jetson AV1 publishing for WebRTC by disabling IVF frame headers, validating low-overhead OBU output, caching and prepending sequence-header OBUs on keyframes, and treating empty MMAPI capture buffers as encode failures.
