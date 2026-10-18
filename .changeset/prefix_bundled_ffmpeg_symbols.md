---
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
webrtc-sys: patch
---

Prefix every ffmpeg symbol bundled in the linux `libwebrtc.a` with `LK_`, so an application that also links its own FFmpeg, shared or static, no longer calls libwebrtc's copy or fails to link.
