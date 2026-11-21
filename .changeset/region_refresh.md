---
livekit-signaling: patch
livekit-region: patch
livekit: patch
livekit-api: patch
livekit-capture: patch
livekit-ffi: patch
---

A connected signal session on a LiveKit Cloud URL now refetches the region list whenever it goes stale, using the latest token, so a region failover after a long call does not wait for a fetch.
