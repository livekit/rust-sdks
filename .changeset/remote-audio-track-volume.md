---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
---

Add `RemoteAudioTrack::set_volume` (backed by `RtcAudioTrack::set_volume` and WebRTC's `AudioSourceInterface::SetVolume`) to change a remote participant's playout volume locally.
