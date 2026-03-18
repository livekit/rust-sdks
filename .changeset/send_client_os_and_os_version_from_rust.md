---
livekit: patch
webrtc-sys: patch
livekit-wakeword: patch
webrtc-sys-build: patch
livekit-protocol: patch
livekit-ffi: patch
libwebrtc: patch
soxr-sys: patch
yuv-sys: patch
livekit-api: patch
imgproc: patch
---

# Send client os and os_version from rust

#952 by @MaxHeimbrock

Adds [os_info](https://crates.io/crates/os_info) crate as dependency and sends the data for client connections.
