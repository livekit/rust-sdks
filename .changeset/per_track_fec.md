---
livekit: minor
livekit-ffi: minor
libwebrtc: minor
webrtc-sys: minor
webrtc-sys-build: patch
---

Negotiate FlexFEC for every published video track and choose send-side protection per track (disabled, low, medium, or high), applied through the new `RtpParameters::fec_protection` sender policy.
