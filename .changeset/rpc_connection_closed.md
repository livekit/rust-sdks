---
livekit-rpc: major
livekit: major
livekit-ffi: patch
livekit-capture: patch
---

When the local room closes, in-flight `perform_rpc` calls now fail with `RpcErrorCode::ConnectionClosed` (1506) instead of `RecipientDisconnected` (1503), which remains reserved for remote participant departures.
