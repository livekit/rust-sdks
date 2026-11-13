---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
---

Add `RemoteAudioTrack::set_volume` and `RtcAudioTrack::set_volume` to set a remote track's playout volume on this device only, applied on WebRTC's signaling thread and clamped to the 0–10 range it accepts.
