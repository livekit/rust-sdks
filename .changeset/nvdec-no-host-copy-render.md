---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
---

Emit Linux NVDEC frames with zero display reordering as native CUDA NV12 buffers and render them through synchronized CUDA/Vulkan external memory without CPU frame copies, with automatic I420 fallback.
