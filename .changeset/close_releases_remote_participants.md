---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Fix remote participant and track leaks when closing a room, including encrypted tracks, cancelled closure, and participant updates racing with teardown.
