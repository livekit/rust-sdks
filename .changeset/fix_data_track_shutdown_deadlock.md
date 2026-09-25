---
livekit-datatrack: patch
livekit-ffi: patch
livekit-uniffi: patch
livekit: patch
---

Fix a data-track shutdown deadlock on room disconnect by cancelling both managers at the start of close, and by racing their output sends and in-flight RTC forwarding against that cancellation so shutdown cannot wait on a full queue or a stuck signal or reconnection.
