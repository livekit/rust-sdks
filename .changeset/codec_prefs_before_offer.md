---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Set video codec preferences immediately after adding the transceiver so an offer cannot be created with the default VP8-first order before an H.264 publish applies its preferences.
