---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

On a full reconnect, `Room` now reports `Reconnecting` (and the `ConnectionStateChanged` that precedes it) before removing remote participants, so the `ParticipantDisconnected` events of a full reconnect arrive after `Reconnecting` rather than before it and can be told apart from participants actually leaving. #1591 (@MaxHeimbrock)
