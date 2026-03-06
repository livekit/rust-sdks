---
yuv-sys: patch
imgproc: patch
webrtc-sys: patch
soxr-sys: patch
livekit-ffi: patch
livekit-protocol: patch
libwebrtc: patch
livekit: patch
livekit-api: patch
webrtc-sys-build: patch
---

# Fix H.264 codec matching

#931 by @ladvoc

Addresses an issue with H.264 streams using `packetization-mode=0`. This affects tracks published between two cloud regions and all video tracks published by the [ESP32 SDK](https://github.com/livekit/client-sdk-esp32).

**Root cause**: the platform hardware decoder is not recognized as supporting H.264 streams with `packetization-mode=0` because the codec matching logic treats different packetization modes as entirely distinct codecs. As a result, no compatible decoder is found and none is created. Packets are received but have no decoder to process them, so no frames are ever emitted to the user. Other SDKs (Swift, web, etc.) did not have the same issue.
