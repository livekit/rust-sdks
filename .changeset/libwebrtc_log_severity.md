---
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Log libwebrtc messages at their own severity instead of reporting every one as debug, so its warnings and errors are no longer filtered out by default in every SDK.
