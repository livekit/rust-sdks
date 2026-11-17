---
livekit: minor
livekit-signaling: minor
livekit-api: minor
livekit-capture: patch
livekit-ffi: patch
---

Add `RoomOptions::publish_tracks`, a list of tracks and their publish options published while connecting and sent with the join request in single peer connection mode to save the round trips of a separate `AddTrackRequest` and SDP negotiation; `livekit-signaling`'s `SignalClient::connect` now takes an `InitialPublish` instead of a bare publisher offer, and `RtcEngine::connect` takes the tracks to publish with the join. Local tracks now keep the WebRTC runtime that created them alive, so tracks created before connecting stay usable in the room; call `enable_zero_playout_delay` before creating local tracks, since it fails once the runtime is active. Publishing a track now fails as soon as the server refuses it instead of after the publish timeout, and a track muted before or while it is published stays muted instead of being re-enabled.
