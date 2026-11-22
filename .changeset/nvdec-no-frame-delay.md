---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

NVDEC outputs each frame with its own access unit instead of one frame late, NVDEC H.264 falls back to the software decoder on errors, and H.265 follows size changes at keyframes.
