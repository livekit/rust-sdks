---
livekit: minor
libwebrtc: patch
webrtc-sys: patch
---

# Add per-track FlexFEC protection levels

Enable FlexFEC receiver negotiation automatically and replace the global
protection parameters with `RoomOptions::fec_enabled` plus per-track disabled,
low, medium, and high protection levels. FlexFEC now always uses a one-frame
block and a random-loss mask.
