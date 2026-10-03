---
livekit: patch
livekit-ffi: patch
livekit-capture: patch
---

Publish a 2-channel audio track as stereo. `AddTrackRequest` never carried `TF_STEREO`, so the
server negotiated mono Opus and a stereo `NativeAudioSource` reached subscribers with identical
L and R channels. The track is now flagged stereo when its source has two channels, matching
the JS SDK.
