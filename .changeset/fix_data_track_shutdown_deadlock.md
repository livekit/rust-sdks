---
livekit-datatrack: patch
livekit-ffi: patch
livekit-uniffi: patch
livekit: patch
---

Fix a data track manager deadlock during room disconnect. Shutdown is now signaled
via a `CancellationToken` (with child tokens for track tasks) instead of the bounded
event channel, so it cannot be dropped when in-flight track events saturate the
channel. `InputEvent::Shutdown` is removed; use [`ManagerInput::shutdown`].
