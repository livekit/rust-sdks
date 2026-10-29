---
livekit: minor
livekit-signaling: minor
livekit-api: patch
livekit-capture: patch
livekit-ffi: patch
---

Add `RoomOptions::publish_tracks`, a list of `InitialTrack`s published while connecting and sent with the join request in single peer connection mode to save the round trips of a separate `AddTrackRequest` and SDP negotiation; `livekit-signaling`'s `SignalClient::connect` now takes an `InitialPublish` instead of a bare publisher offer, and `RtcEngine::connect` is now internal to the SDK. Local tracks now keep the WebRTC runtime that created them alive, so tracks created before connecting stay usable in the room.
