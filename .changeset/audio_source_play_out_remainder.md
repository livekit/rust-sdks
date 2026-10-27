---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
---

Play out audio queued in a `NativeAudioSource` when less than 10 ms remains, padded with silence, instead of holding it until the next `capture_frame`. Previously the end of a push that didn't finish on a 10 ms boundary played only when the next push arrived, or never.
