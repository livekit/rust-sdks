---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
---

Add an opt-in zero-playout-delay mode for native video subscribers, expose it through the `local_video` subscriber's `--low-latency` flag, and isolate subscriber diagnostics from frame-driven video rendering.
