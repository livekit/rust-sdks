---
livekit-rpc: major
livekit: major
livekit-ffi: patch
livekit-capture: patch
---

When the local room closes, in-flight `perform_rpc` calls now fail with the new built-in `RpcErrorCode::ConnectionClosed` (1506, "Connection closed") instead of `RecipientDisconnected` (1503). `RecipientDisconnected` now only means the remote participant left the room, matching the JS SDK. Code that matches `RpcErrorCode` exhaustively needs a case for the new variant, and callers that checked for 1503 to detect their own disconnect should check for 1506.
