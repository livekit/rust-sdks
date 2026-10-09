---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
---

Keep pre-encoded video decodable when WebRTC drops frames: the passthrough encoder disables WebRTC's frame dropper, and after a frame is dropped before it, holds delta frames and requests a keyframe until the next keyframe arrives.
