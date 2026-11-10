---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
---

Fall back to software encoding when an NVENC, VAAPI or Jetson hardware encoder fails to initialize or fails mid-stream, instead of leaving the track without video.
