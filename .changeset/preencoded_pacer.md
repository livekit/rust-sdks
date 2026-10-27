---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Read WebRTC field trials from `LIVEKIT_WEBRTC_FIELD_TRIALS` (for example `WebRTC-Video-Pacing/factor:10/`) and stop capping the starting bandwidth estimate of pre-encoded tracks, so their packets no longer queue in the pacer after publishing.
