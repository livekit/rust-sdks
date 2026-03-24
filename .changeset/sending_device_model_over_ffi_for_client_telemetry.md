---
webrtc-sys-build: patch
yuv-sys: patch
livekit-protocol: patch
livekit-ffi: patch
soxr-sys: patch
livekit-api: patch
livekit-wakeword: patch
libwebrtc: patch
livekit: patch
livekit-datatrack: patch
imgproc: patch
webrtc-sys: patch
---

# Sending device model over ffi for client telemetry

#966 by @MaxHeimbrock

This is probably a breaking change. 

Send device_model if set from ffi, otherwise send "Unknown"

Alternatively we could add it to some room options proto, but this follows the same pattern as sdk and sdk version, which is also client telemetry and set in the `LiveKitInitialize` of the ffi.

Enables: https://github.com/livekit/client-sdk-unity/pull/201
