---
livekit: minor
livekit-ffi: minor
libwebrtc: minor
webrtc-sys: minor
webrtc-sys-build: patch
livekit-protocol: patch
livekit-api: patch
---

Negotiate FlexFEC for every published video track and choose send-side protection per track with a mode (disabled, adaptive, fixed, or minimum) and a level (low, medium, or high) that mirror `FECProtection` in the LiveKit protocol, applied through the new `RtpParameters::fec_protection` sender policy.
