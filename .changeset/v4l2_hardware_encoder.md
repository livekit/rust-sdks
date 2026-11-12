---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
livekit-hwcodec: patch
---

Adds H.264 hardware encoding on 64-bit Linux through V4L2 stateful encoders such as Qualcomm Dragonwing's, used automatically or with `VideoEncoderBackend::Hardware`, with fallback to software encoding if the encoder fails.
