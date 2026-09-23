---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Pre-encoded passthrough encoder declares a trusted rate controller, so the frame dropper no longer discards pre-encoded delta frames
