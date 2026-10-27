---
libwebrtc: patch
webrtc-sys: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---
Reading a transceiver's current or fired direction before it is negotiated returns `None` instead of aborting the process.
