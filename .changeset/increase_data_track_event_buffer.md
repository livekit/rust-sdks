---
livekit-datatrack: patch
livekit: patch
livekit-uniffi: patch
livekit-capture: patch
livekit-ffi: patch
---

Increase the local and remote data track event buffers so a burst of track lifecycle events cannot fill the channel and deadlock room disconnect.
