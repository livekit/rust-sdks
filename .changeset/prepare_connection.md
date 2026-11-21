---
livekit: minor
livekit-signaling: minor
livekit-ffi: minor
livekit-api: patch
livekit-capture: patch
---

Add `Room::prepare_connection` and the FFI `PrepareConnectionRequest`, which pick the best region of a LiveKit Cloud project before connecting and keep the region list fresh until then, so that the next connect to the same URL dials that region directly; `livekit-signaling` adds `RegionUrlProvider::prepare` for the same purpose.
