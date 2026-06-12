---
libwebrtc: minor
livekit: patch
livekit-ffi: patch
---

Add dynacast support - #1003 (@chenosaurus, @stephen-derosa)

This includes a minor breaking change for `libwebrtc`: `RtpParameters` now
contains additional RTP sender state that must be preserved when round-tripping
through `set_parameters()`.
