---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
---

Fix native video-source lifecycle and NVENC initialization failure handling.

The raw-video keepalive task now uses a weak liveness check and defers its
black I420 buffer allocation until source liveness is confirmed, so dropping
an unused source releases its resources. `nvEncInitializeEncoder` failures now
propagate instead of leaving the encoder half-initialized.
