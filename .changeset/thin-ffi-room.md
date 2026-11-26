---
livekit-ffi: minor
---

Replace NativeRoomSession with FfiRoom, directly await core commands through UniFFI, and read snapshot values from core handles while preserving ordered events and connection cleanup.
