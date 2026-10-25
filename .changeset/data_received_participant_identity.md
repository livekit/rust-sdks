---
livekit: minor
livekit-ffi: minor
---

`RoomEvent::DataReceived` now carries `participant_identity`, the sender identity stamped on the packet by the server, so data from a participant the room does not know yet (its announcement is still in flight) can be attributed instead of dropped. The FFI's `DataPacketReceived.participant_identity` is filled from it in that case instead of being left empty.
