---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

`Room::close` now leaves the room `Disconnected` and emits `RoomEvent::Disconnected` before it returns. The room's event task could drop the engine's `Disconnected` event when the close signal won the race, so a closed room kept reporting `Connected` (or `Reconnecting`) about half the time.
