---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
livekit-uniffi: patch
livekit-capture: patch
---

Decode H.265 in hardware on Jetson through the Jetson Multimedia API (NVDEC), which also lets Jetson publishers negotiate H.265 with single peer connection enabled; set `LK_DISABLE_JETSON_DEC` to use the software decoders instead.
