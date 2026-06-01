---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
---

Add native Jetson H.264 hardware decoding through the Jetson Multimedia API and
V4L2 capture-plane DMA-BUF frames. The `local_video` subscriber can import those
DMA-BUF frames into Vulkan textures for zero-copy decode-to-render when the
driver supports the exported layout.
