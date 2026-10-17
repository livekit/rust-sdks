---
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
webrtc-sys: patch
---

Fix the VAAPI H.264 encoder leaking VA parameter buffers on every frame; they are now destroyed after vaEndPicture().
