---
livekit: patch
livekit-ffi: patch
---

Break two arc cycles between RoomSession and itself and DataChannel and itself introduced via closures.