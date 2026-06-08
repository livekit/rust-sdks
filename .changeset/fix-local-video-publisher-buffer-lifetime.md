---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
---

Fix NVIDIA encoder I420 uploads to copy each plane using its actual source stride, avoiding chroma corruption when source frames use padded YUV planes. Also fix the `local_video` publisher reusing mutable I420 frame storage after handing frames to WebRTC.
