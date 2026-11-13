---
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
webrtc-sys: patch
---

Fix `NativeBuffer::get_cv_pixel_buffer` on macOS keeping every frame it was asked for alive until the calling thread exits, which stopped VideoToolbox decoding after 16384 frames.
