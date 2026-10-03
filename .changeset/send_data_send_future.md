---
livekit: patch
livekit-api: patch
livekit-capture: patch
livekit-ffi: patch
---

RoomClient::send_data now returns a Send future by dropping its RNG before awaiting.
