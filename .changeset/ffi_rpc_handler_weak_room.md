---
livekit-ffi: patch
---

Fix a room leaking after `dispose()` once an RPC method has been registered.

The handler registered for an RPC method captured the room strongly, and it is stored on
that same room's RPC server, so the two formed a cycle that nothing unregistered during
teardown. A single registered method made the room outlive `dispose()`, keeping the engine,
its peer connections and the WebRTC runtime resident for the rest of the process. The
handler now captures the room weakly. Closing a room also fails any RPC invocation still
awaiting a response, which would otherwise hold the room open indefinitely because the
client can no longer answer once its handles are gone.
