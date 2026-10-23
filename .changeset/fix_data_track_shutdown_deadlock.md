---
livekit-datatrack: patch
livekit-ffi: patch
livekit-uniffi: patch
livekit: patch
---

Fix a data track manager deadlock during room disconnect. Shutdown was delivered
through the bounded event channel with `try_send`, so it was silently dropped
whenever in-flight track events had saturated the channel, leaving the manager
task and every caller awaiting disconnect stranded.
