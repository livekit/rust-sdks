---
webrtc-sys: minor
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

NVENC and NVDEC are built on Windows when `CUDA_HOME` holds the CUDA headers; the NVIDIA libraries are loaded at run time.
