---
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
webrtc-sys: patch
---

Preserve distinct frame metadata when queued video frames are captured within the same millisecond, including pre-encoded frames that share an RTP timestamp.
