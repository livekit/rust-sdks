---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
---

Fix H.264, H.265, and AV1 NVENC sessions so live bitrate and frame rate updates
reconfigure the hardware without restarting the encoder.
