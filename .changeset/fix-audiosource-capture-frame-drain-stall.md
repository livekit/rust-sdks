---
libwebrtc: patch
livekit: patch
livekit-ffi: patch
webrtc-sys: patch
---

Bound the buffered `AudioSource::capture_frame` completion wait and split the drain lock so a stalled or wedged source drain returns a recoverable error instead of hanging the producer (and the session) forever (#408, #420, #497) - #1289 (@sam-hark)
