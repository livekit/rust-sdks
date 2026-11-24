---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

The NVENC H.264 encoder uses the negotiated profile; its profile GUID was uninitialized.
