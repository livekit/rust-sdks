---
livekit-ffi: minor
livekit-telemetry: minor
---

Limit the FFI lifecycle to three admitted telemetry generations and return AtCapacity before constructing a rejected replacement; pulled configuration now returns a Result and callers must keep serving previous queues when replacement fails.
