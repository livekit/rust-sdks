---
libwebrtc: patch
livekit: patch
livekit-ffi: patch
webrtc-sys: patch
---

Emit black keepalive frames from NativeVideoSource instead of uninitialized memory. webrtc::I420Buffer::Create leaves the pixel planes uninitialized, so the pre-capture keepalive frames could leak recycled heap contents (often fragments of earlier frames from the same process) to subscribers as the first keyframes - #1271 (@eh-steve)
