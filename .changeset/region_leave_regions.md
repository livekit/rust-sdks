---
livekit-signaling: patch
livekit: patch
livekit-api: patch
livekit-capture: patch
livekit-ffi: patch
---

The region list that a LiveKit Cloud server sends with a leave request now goes into the region cache, so the reconnect that follows can fail over to another region without fetching the list first.
