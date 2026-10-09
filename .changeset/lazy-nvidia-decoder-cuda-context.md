---
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
webrtc-sys: patch
---

Query NVIDIA decoder capabilities without eagerly creating a CUDA context, avoiding unnecessary context churn for rooms that do not decode with NVDEC.
