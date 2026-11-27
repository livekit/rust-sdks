---
livekit: patch
livekit-signaling: minor
livekit-ffi: minor
livekit-region: patch
---

Add `Room::prepare_connection` and the FFI `PrepareConnectionRequest`, which pick the best region of a LiveKit Cloud project before connecting and keep the region list fresh until then, so that the next connect to the same URL dials that region directly; `Room::cancel_prepared_connection` undoes it, and `livekit-signaling` adds `RegionUrlProvider::prepare` and `RegionUrlProvider::cancel_prepared` for the same purpose.
