---
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
webrtc-sys: patch
---

Keep NVIDIA's Jetson video codec plugin loaded while probing MMAPI encoder capabilities, preventing probe teardown from breaking subsequent GStreamer capture.
