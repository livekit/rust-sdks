---
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
webrtc-sys: patch
---

Avoid creating and destroying a temporary Jetson MMAPI encoder during capability discovery, preventing NVIDIA plugin teardown from breaking subsequent GStreamer capture.
