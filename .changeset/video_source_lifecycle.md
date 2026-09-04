---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
---

Fix NativeVideoSource keepalive retaining dropped sources.

The raw-video keepalive task now uses a weak liveness check instead of cloning
`NativeVideoSource`, so dropping an unused source releases the native handle
and black I420 keepalive buffer. `nvEncInitializeEncoder` failures now
propagate instead of leaving the encoder half-initialized.
