---
webrtc-sys: patch
---

Add `LK_DISABLE_VIDEOTOOLBOX_DECODER` so benchmark runs can bypass the Apple platform video decoder and fall back to software decode.
