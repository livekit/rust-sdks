---
livekit-signaling: patch
livekit: patch
livekit-api: patch
livekit-capture: patch
livekit-ffi: patch
---

A connect that the server rejects with a 401 no longer retries on the other regions of a LiveKit Cloud project, and an expired region list is no longer used after the region endpoint rejects the token.
