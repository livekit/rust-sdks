---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Preserve `RtpSender::set_track(None)` by clearing the native sender without dereferencing a null track, and allow a replacement track to be attached afterward.
