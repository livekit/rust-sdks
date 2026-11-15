---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Ignore a duplicated server answer instead of applying it a second time, which libwebrtc rejects and which surfaced as an error for a renegotiation that had actually succeeded.
