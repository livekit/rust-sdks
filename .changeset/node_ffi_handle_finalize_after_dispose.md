---
livekit-ffi: patch
---

Stop the Node bindings' `FfiHandle` finalizer from throwing "trying to drop an invalid handle" when a handle is garbage-collected after `livekitDispose()`.

Since `FfiServer::dispose` started dropping every remaining handle (#1395), a JS `FfiHandle` that outlived the dispose — any room, participant, track or similar object the app never disposed explicitly — would find its handle already gone when GC later finalized it. The finalizer turned that into an error, which Node raises as an uncaught exception wherever GC happens to run. A handle that misses after the server is disposed is now treated as already released, and the finalizer no longer propagates errors at all.
