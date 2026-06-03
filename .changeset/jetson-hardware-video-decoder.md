---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
---

Add native Jetson H.264 hardware decoding through the Jetson Multimedia API and
V4L2 capture-plane DMA-BUF frames. The `local_video` subscriber can import those
DMA-BUF frames into Vulkan textures for zero-copy decode-to-render when the
driver supports the exported layout. Set `LIVEKIT_VIDEO_DECODER=software` or run
the subscriber with `--video-decoder software` to skip platform hardware decoder
probes and use software decoding.
