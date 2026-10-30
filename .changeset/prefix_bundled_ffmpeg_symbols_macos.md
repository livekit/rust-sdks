---
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
webrtc-sys: patch
---

Prefix every ffmpeg symbol bundled in the macOS `libwebrtc.a` with `LK_`, as the linux one is, so an application that also links its own FFmpeg no longer calls libwebrtc's copy.
