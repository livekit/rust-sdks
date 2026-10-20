---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
livekit-uniffi: patch
livekit-capture: patch
---

Fix a hang creating a `PeerConnectionFactory` on Jetson builds, where unloading NVIDIA's `libnvbufsurftransform` after the hardware encoder probe deleted WebRTC's thread-local key.
