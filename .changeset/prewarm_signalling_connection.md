---
livekit-net: minor
livekit-signaling: minor
livekit: minor
livekit-ffi: minor
livekit-api: patch
livekit-capture: patch
livekit-telemetry: patch
livekit-token-source: patch
---

Add `PrewarmedConnection` and `RoomOptions::prewarmed_connection` to open the signalling TCP/TLS connection ahead of `Room::connect`, so the connect only performs the WebSocket upgrade; exposed over FFI through `PrewarmRequest` and `RoomOptions.prewarmed_connection_handle`.
