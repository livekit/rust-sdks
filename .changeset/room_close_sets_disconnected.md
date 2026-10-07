---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

`Room::close` now leaves the room `Disconnected` and emits `RoomEvent::Disconnected` before it returns, and `Disconnected` is final: a reconnect still finishing in the background (a queued resume, or the republish after a full reconnect) can no longer bring a closed room back to `Connected` or emit `Reconnected` after `Disconnected`. Before, the room's event task could drop the engine's `Disconnected` event when the close signal won the race, so a closed room kept reporting `Connected` (or `Reconnecting`) about half the time.
