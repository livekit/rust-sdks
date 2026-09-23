---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
---

Advertise pre-encoded pass-through video formats even when no local encoder implements the codec, so pre-encoded H.265 publishes negotiate on machines without a hardware HEVC encoder.
