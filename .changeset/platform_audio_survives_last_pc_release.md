---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
---

PlatformAudio keeps working after the last PeerConnection is released. AdmProxy no longer terminates its sub ADMs when the media engine terminates it, and a negative device count is treated as zero.
